# AUDIT LOCAL DE RÉ-ÉVALUATION POST-CORRECTIFS
**Date** : 2026-11-18 17:53:41
**Modèle** : `qwen/qwen3.6-35b-a3b` (0 token cloud)

---

## 1. Crate `rraw-gpu`
> [Raisonnement de l'auditeur]
Here's a thinking process:

1.  **Analyze User Input:**
   - **Role:** Inquisiteur en Chef du projet Rraw (Chief Inquisitor of the Rraw project)
   - **Task:** Evaluate recent performance fixes, check for remaining flaws/bottlenecks in critical paths.
   - **Output Format:** Markdown, no preamble, exactly three sections:
     ### 1. Verdict sur les Correctifs Récents
     ### 2. Points Faibles ou Goulots Restants
     ### 3. Note Révisée (/10) et Recommandation Finale
   - **Input Code:** A Rust implementation of an LRU VRAM Cache (`LruVramCache`) using `BTreeMap` and `HashMap`. It tracks `photo_id`, `size_bytes`, and uses a `seq_counter` for LRU ordering. Methods: `new`, `current_vram_usage`, `max_vram_budget`, `contains`, `touch`, `insert_mock_photo`, `evict`.
   - **Goal of the pass:** Verify LRU VRAM cache (BTreeMap + HashMap, access and eviction O(log N)).

2.  **Code Analysis (Mental Walkthrough & Critique):**
   - *Data Structures:* `BTreeMap<u64, String>` for `access_order`, `HashMap<String, (VramPhotoEntry, u64)>` for `entries`.
   - *Complexity Claims:* The docstring says "indexed sequence order for O(log N) touch and eviction without linear scans." Let's verify.
     - `touch`: `self.entries.get_mut(photo_id)` is O(1) avg. `self.access_order.remove(old_seq)` is O(log N). `self.access_order.insert(...)` is O(log N). So `touch` is O(log N). Correct.
     - `evict`: `self.entries.remove(photo_id)` is O(1) avg. `self.access_order.remove(&seq)` is O(log N). So `evict` is O(log N). Correct.
     - `insert_mock_photo`: Handles removal, then a `while` loop for eviction. Each eviction step removes from both maps: O(log N) + O(1) avg. The loop runs K times for K evictions. So worst-case O(K log N). This is standard for LRU.
     - `contains`: O(1) avg.
   - *Memory/Performance Issues:*
     - `String` allocations in `access_order`: `BTreeMap<u64, String>` stores a `String` for every entry. This means every cache entry allocates a `String` just for ordering. This is highly inefficient. A better approach would be to store `&str` or use an index/ID, or better yet, use `std::collections::VecDeque` or `dashmap`/`moka` for production, but given the constraints, storing `String` here is a memory bloat and allocation bottleneck. Actually, `BTreeMap` requires owned keys/values. We could use `&'static str` if IDs were static, but they aren't. Alternatively, use a custom structure or `Arc<str>`, but `String` is fine for now, though it adds allocation overhead on every `touch` and `insert`.
     - Wait, `self.access_order.insert(self.seq_counter, photo_id.to_string());` allocates a new `String` on every touch/insert. This is a significant allocation bottleneck in a hot path (VRAM cache touches happen frequently during navigation).
     - `self.entries` stores `(VramPhotoEntry, u64)`. `VramPhotoEntry` contains `photo_id: String`. So `photo_id` is stored twice: once in `entries`, once in `access_order`. Double allocation/storage.
     - `saturating_add`/`saturating_sub`: Good for safety, but adds branching. In performance-critical code, unchecked arithmetic might be preferred if overflow is impossible by design, but it's fine.
     - `insert_mock_photo` name: Suggests it's for testing. Should probably be `insert` or `put`.
     - Concurrency: No `Mutex`/`RwLock`. The struct is `!Sync`. If used across threads, it will panic or require wrapping. The prompt doesn't specify concurrency, but GPU/VRAM contexts often are multi-threaded. I'll note it.
     - LRU Implementation Flaw: The `BTreeMap` approach is actually an O(log N) LRU, but it's not the most efficient. A `VecDeque` + `HashMap` is O(1) for touch/eviction. The docstring claims O(log N), which is true, but it's suboptimal compared to O(1) alternatives. However, the prompt says "Vérifier le cache LRU VRAM (BTreeMap + HashMap, accès et éviction O(log N))", so they intentionally chose O(log N). I'll evaluate it as requested but point out the O(1) alternative if critical.
     - Memory Leak/Inconsistency? Let's trace `touch`:
       ```rust
       if let Some((_, old_seq)) = self.entries.get_mut(photo_id) {
           self.access_order.remove(old_seq);
           self.seq_counter = self.seq_counter.saturating_add(1);
           *old_seq = self.seq_counter;
           self.access_order.insert(self.seq_counter, photo_id.to_string());
       }
       ```
       This works. But note: `self.entries.get_mut(photo_id)` returns a reference to `(VramPhotoEntry, u64)`. We only use the `u64` (old_seq). We don't update the `VramPhotoEntry`. That's fine.
     - `insert_mock_photo` eviction loop:
       ```rust
       while self.current_vram_usage.saturating_add(size_bytes) > self.max_vram_budget_bytes
           && !self.access_order.is_empty()
       {
           if let Some((&oldest_seq, _)) = self.access_order.first_key_value() {
               if let Some(oldest_id) = self.access_order.remove(&oldest_seq) {
                   if let Some((evicted, _)) = self.entries.remove(&oldest_id) {
                       self.current_vram_usage = self.current_vram_usage.saturating_sub(evicted.size_bytes);
                   }
               }
           } else {
               break;
           }
       }
       ```
       This is correct but verbose. `first_key_value()` returns `Option<(&K, &V)>`. Destructuring `(&oldest_seq, _)` is fine. Then `remove(&oldest_seq)` is O(log N). Then `entries.remove(&oldest_id)` is O(1). Then update usage. It's correct.
     - Potential bug: `self.access_order.remove(old_seq)` in `touch` uses `old_seq` which is a `u64`. But `access_order` is `BTreeMap<u64, String>`. So `remove` takes `&u64`. `old_seq` is `u64` (copied from tuple). It works due to `Borrow` trait, but it's better to be explicit or use `&old_seq`. Actually, `remove` takes `&Q` where `K: Borrow<Q>`. `u64` implements `Borrow<u64>`, so it's fine.
     - Another thing: `self.seq_counter` is `u64`. It will wrap around after 2^64 touches. `saturating_add` prevents panic but breaks LRU ordering eventually (old entries might get new low counters). This is a known issue with sequence counters. Should use wrapping or a different approach, or just note it. For a cache, it's usually fine if N << 2^64, but worth mentioning.
     - Memory overhead: `BTreeMap` nodes have overhead. `HashMap` has overhead. Double storage of `photo_id` (String) is the biggest red flag.
     - `insert_mock_photo` vs `insert`: Naming is weird. I'll note it.
     - Thread safety: Not mentioned. I'll assume single-threaded for now but flag it.

3.  **Structure the Evaluation (per prompt):**
   - **1. Verdict sur les Correctifs Récents:** Acknowledge the O(log N) claim is met. Praise the separation of concerns, safe arithmetic (`saturating_*`), and correct eviction logic. Note that the BTreeMap+HashMap combo achieves the stated complexity but introduces allocation overhead.
   - **2. Points Faibles ou Goulots Restants:** 
     - Double allocation of `photo_id` (String in both maps).
     - Allocation in hot path (`touch` and `insert` do `.to_string()`).
     - `seq_counter` saturation/wrapping issue over long uptimes.
     - `insert_mock_photo` naming suggests test-only, but logic is production-ready.
     - Lack of concurrency primitives (`Mutex`/`RwLock`) if used in async/GPU contexts.
     - `BTreeMap` node allocation overhead vs `VecDeque`/`LinkedList` for O(1) LRU.
   - **3. Note Révisée (/10) et Recommandation Finale:** Give a score (e.g., 7.5/10). Recommend: deduplicate storage, use `Arc<str>` or indices, consider `VecDeque` for O(1), fix naming, add thread-safety wrapper if needed, handle counter wrapping.

4.  **Draft the Content (Mental Refinement in French):**
   *(Section 1)* Le correctif respecte l'objectif architectural : la complexité temporelle est bien maîtrisée à O(log N) pour les opérations critiques (`touch`, `evict`, `insert`). La séparation entre la vue d'ordre (`BTreeMap`) et le stockage des données (`HashMap`) est logique. L'usage de `saturating_add`/`saturating_sub` garantit la robustesse face aux dépassements, et la boucle d'éviction est correcte dans son principe. Cependant, l'implémentation souffre d'un surcoût mémoire et allocationnel non négligeable pour un cache VRAM à haute fréquence d'accès.

   *(Section 2)* 
   - **Double allocation de `photo_id`** : La chaîne est dupliquée dans `entries` et `access_order`. Chaque `touch` ou `insert` provoque une allocation `String` supplémentaire dans le `BTreeMap`, créant un goulot en allocation mémoire (GC pressure en Rust = heap churn).
   - **Goulot dans le chemin critique (`touch`)** : `photo_id.to_string()` est appelé à chaque rafraîchissement LRU. En navigation rapide, cela génère des allocations CPU inutiles.
   - **Saturation du compteur** : `seq_counter` utilise `saturating_add`. Après ~18 quintillions d'accès, le compteur se fige, brisant la logique LRU. Un compteur `wrapping_add` ou une approche par timestamp/epoch est préférable.
   - **Nommage trompeur** : `insert_mock_photo` suggère un code de test, mais contient la logique de production. Risque de confusion ou d'oubli lors de la migration.
   - **Absence de synchronisation** : Le type n'est pas `Sync`. Si le cache est partagé entre threads (contexte GPU/async), une enveloppe `RwLock` ou `Mutex` est obligatoire, ce qui ajoutera une contention non évaluée ici.
   - **Complexité sous-optimale** : Bien que O(log N) soit conforme à la demande, une `VecDeque` couplée à un `HashMap` permettrait un O(1) réel pour `touch`/`evict`, réduisant drastiquement la latence p99.

   *(Section 3)* Note : 7.5/10. Recommandation : Fusionner les structures pour éviter la duplication (ex: `HashMap<String, (VramPhotoEntry, u64)>` + `VecDeque<String>` pour l'ordre, ou utiliser des indices/`Arc<str>`). Remplacer `saturating_add` par `wrapping_add` ou un mécanisme de recyclage de séquence. Renommer `insert_mock_photo` en `insert` ou `put`. Ajouter une couche `RwLock` si le contexte est multithread. Prioriser une structure O(1) si la latence VRAM est critique. Le code est fonctionnel et sûr, mais doit être optimisé pour réduire la pression heap avant déploiement en production.

5.  **Self-Correction/Verification during drafting:**
   - Check complexity claims: `BTreeMap` operations are indeed O(log N). The prompt explicitly says "Vérifier le cache LRU VRAM (BTreeMap + HashMap, accès et éviction O(log N))". So I should acknowledge it meets the spec but point out O(1) alternatives as a bottleneck.
   - Double allocation: `access_order` stores `String`. `entries` stores `VramPhotoEntry` which contains `String`. Yes, double allocation. This is a valid critique.
   - `touch` does `photo_id.to_string()`. Correct.
   - `seq_counter` saturation: `saturating_add` stops incrementing. This breaks LRU. `wrapping_add` is better, or just let it wrap naturally. I'll note it.
   - Naming: `insert_mock_photo` is definitely weird.
   - Thread safety: Important for GPU contexts.
   - Structure matches prompt exactly.
   - Tone: Impitoyable, technique, direct.
   - Format: Markdown, exactly the

---

## 2. Crate `rraw-color`
### 1. Verdict sur les Correctifs Récents
Le hissage de l'invariant `origin` vers `is_red_row` par ligne est partiellement réussi et réduit effectivement la désynchronisation de prédiction de branche pour la parité des rangées. Les constantes `exposure_scale`, `sat_r/g/b` et les multiplicateurs WB sont correctement calculés une seule fois. La fermeture `sample_at` est bien extraite des boucles, évitant la recréation de contextes fermés à chaque itération.

Cependant, le travail reste superficiel sur le chemin critique. `pattern.color_at(x, y)` persiste dans la boucle interne, forçant un dispatch par pixel qui annule les bénéfices du LICM (Loop-Invariant Code Motion) sur la détermination chromatique. Le compilateur ne peut pas vectoriser proprement un `match` dépendant d'un appel de fonction par échantillon. Le hissage est structurel mais pas algorithmique.

### 2. Points Faibles ou Goulots Restants
- **Allocations par frame non négligeables** : `wb_cfa` et `demosaiced` utilisent `Vec::with_capacity` + `push`. Bien que la capacité soit pré-réservée, cela génère une allocation heap par image. Dans un pipeline de référence ou de benchmark, cela introduit de la fragmentation et empêche le réutilisation de buffers (pool

---

## 3. Crate `rraw-core`
### 1. Verdict sur les Correctifs Récents
L'implémentation du masquage `& 1` pour la parité de phase Bayer est **correcte et optimale**. En remplaçant les opérations `% 2` par un ET bitwise, vous éliminez les instructions de division/modulo du pipeline CPU, réduisant la latence d'accès par pixel et facilitant le déroulement de boucle (loop unrolling) ainsi que la vectorisation SIMD en aval. La logique de dispatch sur `(origin, cy, cx)` reste déterministe, branchless sur les coordonnées, et conforme aux spécifications matérielles des mosaïques 2x2. L'usage de `const fn` et `#[must_use]` respecte les principes d'abstraction à coût zéro. Le correctif est validé pour les chemins critiques de démosaïquage et d'indexation CFA.

### 2. Points Faibles ou Goulots Restants
- **X-Trans `% 6`** : La méthode `color_at` utilise `y % 6` et `x % 6` pour le motif X-Trans. Bien que le compilateur optimise les modulos par constantes, `% 6` reste une division entière (ou multiplication par inverse fixe) nettement plus coûteuse que `& 1`. Sur des flux 4K/8K, cela crée un goulot dans les boucles de lecture CFA.
- **Absence d'inlining** : `color_at` et `variance_at` sont appelées des millions de fois par pipeline. Sans `#[inline(always)]`, les appels fonctionnels introduisent une surcharge de stack frame et empêchent l'optimisation cross-crate (LTO) de fusionner les chemins critiques.
- **Branchement dans `variance_at`** : Le test `if normalized_signal.is_finite()` introduit une prédiction de branche. Si le pipeline amont garantit déjà des valeurs finies (ce qui devrait être le cas après normalisation), ce guard est redondant et peut causer des stalls de prédiction en cas de données corrompues ou NaN.
- **Fragmentation mémoire potentielle** : `SignedCfaBuffer::samples` expose un `&[f32]` backed par `Vec<f32>`. Si des réallocations intempestives surviennent lors de la construction (ex: `Vec::with_capacity` non utilisé en amont), vous risquez des allocations cachées et de la fragmentation cache L1/L2.
- **Accès 2D non contigu** : `matrix[y % 6][x % 6]` pour X-Trans crée un double déréférencement. Un tableau plat de 36 éléments avec un index calculé `(y % 6) * 6 + (x % 6)` ou une table de lookup précalculée améliorerait la localité cache.

### 3. Note Révisée (/10) et Recommandation Finale
**Note : 8.5/10**

**Recommandations finales :**
1. Remplacez `% 6` par un index plat ou une table de lookup statique pour X-Trans.
2. Ajoutez `#[inline(always)]` sur `color_at` et `variance_at`.
3. Supprimez le guard `is_finite()` dans `variance_at` si la validation amont est garantie, ou remplacez-le par `normalized_signal.max(0.0).min(f32::MAX)` pour un chemin branchless.
4. Vérifiez que les constructeurs en aval utilisent `Vec::with_capacity(width * height)` pour éliminer toute allocation cachée.
5. Validez la fusion avec LTO activé (`lto = "fat"`) pour garantir que les optimisations cross-crate s'appliquent aux boucles de démosaïquage.

**Verdict :** Correctif de parité validé. Fusion approuvée sous réserve des micro-optimisations hot-path ci-dessus.

---

## 4. Crate `rraw-decode`
> [Raisonnement de l'auditeur]
Here's a thinking process:

1.  **Analyze User Input:**
   - **Role:** Chief Inquisitor of project Rraw.
   - **Task:** Evaluate recent performance fixes, check for remaining flaws/bottlenecks in critical paths.
   - **Output Format:** Direct Markdown evaluation with three specific sections:
     - `### 1. Verdict sur les Correctifs Récents`
     - `### 2. Points Faibles ou Goulots Restants`
     - `### 3. Note Révisée (/10) et Recommandation Finale`
   - **Input Code:** Two Rust files: `lib.rs` (decoder wrapper, EXIF extraction) and `noise_profile.rs` (DNG tag parsing, wavelet MAD noise estimation).
   - **Focus of Audit:** Pre-allocations (`Vec::with_capacity`) for EXIF and MAD wavelet buffers.

2.  **Analyze Code for Performance/Correctness:**
   - **`lib.rs` (EXIF extraction):**
     - `let mut exif_tags = Vec::with_capacity(64);` -> Good pre-allocation. 64 is reasonable for typical EXIF data.
     - Pushes many tags. No reallocation expected.
     - Uses `rawler` for decoding and metadata. The code is straightforward, no obvious performance traps here.
     - String formatting inside loops/conditionals is fine for metadata extraction (not a hot path).
   - **`noise_profile.rs` (Wavelet MAD):**
     - `let raw_pts_per_phase = (width / (step_x * 2)).max(1) * (height / (step_y * 2)).max(1);` -> Calculates potential points.
     - `let stride_mult = (raw_pts_per_phase / 16384).max(1);` -> Caps sampling to ~16k points per channel to avoid huge allocations.
     - `let pts_cap = (raw_pts_per_phase / stride_mult).min(32768);` -> Caps at 32k.
     - `let mut channel_diffs: [Vec<f32>; 3] = [Vec::with_capacity(pts_cap), ...];` -> Pre-allocates exactly `pts_cap` per channel. Good.
     - Sampling loop uses `stride_x` and `stride_y` to skip pixels. This is a smart optimization to bound memory usage and CPU time for large images.
     - `compute_mad_sigma` creates a new `Vec<f32>` for deviations: `let mut deviations: Vec<f32> = values.iter().map(|&v| (v - med).abs()).collect();` -> This allocates a second vector of the same size. Could be optimized to reuse or use a single pass, but it's acceptable for a fallback estimator. However, it's a minor allocation churn.
     - `median` uses `select_nth_unstable_by` -> O(n) average, good. But it mutates the slice. In `compute_mad_sigma`, it mutates `values`, then creates `deviations` from the *original* `values`? Wait, `values` is mutated by `median(values)`. Then `values.iter().map(|&v| (v - med).abs()).collect()` uses the *already mutated* `values` slice! This is a **BUG**. The median function sorts/reorders the slice in-place. After `median(values)`, `values` is no longer in its original order. The subsequent `.iter()` will compute deviations from the wrong values. This is a critical correctness flaw.
     - Let's verify: `median` uses `select_nth_unstable_by`, which partitions the slice. The order of elements is changed. So `values` after `median(values)` is not the original data. The code then does `values.iter().map(|&v| (v - med).abs()).collect()`. This will compute deviations from the permuted array, which is mathematically incorrect for MAD. It should compute deviations from the original values, or clone them first, or compute deviations before sorting.
     - Also, `median` for even-length slices finds the max of the first half and averages with mid. This is correct for median, but `select_nth_unstable_by` doesn't guarantee the first half is sorted, just that all elements <= pivot are before mid. Finding the max in the first half is O(n), which is fine.
     - The bug in `compute_mad_sigma` is severe. It will produce incorrect noise profiles.

3.  **Evaluate Against Audit Objectives:**
   - **Pre-allocations:** Both `exif_tags` (64) and `channel_diffs` (capped at 32k) are correctly pre-allocated. This meets the audit goal.
   - **Algorithmic Gains:** The stride-based subsampling is a solid performance optimization for large images, preventing OOM and excessive CPU time.
   - **Remaining Flaws/Bottlenecks:**
     - Critical bug in `compute_mad_sigma` (mutating slice before computing deviations).
     - Allocation in `compute_mad_sigma` for `deviations` could be avoided by pre-allocating or reusing a buffer, but it's minor compared to the bug.
     - `median` function: `slice.select_nth_unstable_by` is fine, but the even-length median calculation iterates over `slice[..mid]` which is O(n). Acceptable.
     - EXIF extraction: String formatting and many `if let` blocks are fine for metadata, but could be streamlined. Not a performance bottleneck.
     - `rawler` dependency: The code delegates heavy lifting to `rawler`. Performance depends on `rawler`, but that's outside scope.

4.  **Draft Evaluation (Mental Refinement - French):**
   - *Section 1: Verdict* -> Acknowledge correct pre-allocations. `Vec::with_capacity(64)` for EXIF is optimal. Wavelet buffer capping at 32k via stride is a smart memory/CPU bound. Algorithmic compliance is good.
   - *Section 2: Points Faibles* -> Highlight the critical bug in `compute_mad_sigma`: `median` mutates `values` in-place via `select_nth_unstable_by`, but deviations are computed *after* mutation. This breaks the MAD formula. Also note the secondary allocation for `deviations`. Mention that `median`'s even-length handling iterates `slice[..mid]` which is fine but could be optimized. EXIF path is safe but string formatting could be batched (minor).
   - *Section 3: Note & Recommandation* -> Deduct points for the correctness bug. Give a score like 7.5/10 or 8/10. Recommend fixing the `compute_mad_sigma` logic immediately (compute deviations before median or clone), and consider reusing a scratch buffer for `deviations` to eliminate allocation churn.

   Structure the output exactly as requested. Use precise, technical French. No preamble.

   Check the bug again:
   ```rust
   fn compute_mad_sigma(values: &mut [f32]) -> f32 {
       let med = median(values); // MUTATES values
       let mut deviations: Vec<f32> = values.iter().map(|&v| (v - med).abs()).collect(); // USES MUTATED values
       let mad = median(&mut deviations);
       mad / 0.674_489_75
   }
   ```
   Yes, this is definitely wrong. The MAD formula requires absolute deviations from the median of the *original* values. After `median(values)`, `values` is partitioned, not sorted, and elements are moved. The deviations will be computed on a permuted array. This is a critical flaw.

   Also, `median` for even length:
   ```rust
   let prev = slice[..mid]
       .iter()
       .copied()
       .fold(f32::NEG_INFINITY, f32::max);
   (prev + slice[mid]) * 0.5
   ```
   This finds the max of the first half. Since `select_nth_unstable_by` puts all elements <= pivot in the first half, the max of the first half is indeed the element at `mid-1` (if sorted), but since it's not sorted, finding the max is correct to get the lower median. It's O(n). Acceptable.

   I will point out the bug clearly. I will also note that `Vec::with_capacity` is correctly applied, but the algorithmic correctness is compromised.

   Draft:
   ### 1. Verdict sur les Correctifs Récents
   Les pré-allocations demandées sont correctement implémentées. `exif_tags` utilise `Vec::with_capacity(64)`, ce qui élimine les réallocations pour un jeu de métadonnées EXIF standard. Pour le chemin critique du bruit, le mécanisme de sous-échantillonnage par pas (`stride_mult`) couplé à `Vec::with_capacity(pts_cap)` (plafonné à 32 768 éléments par canal) est une optimisation mémoire/CPU rigoureuse. Il empêche les allocations dévastatrices sur les capteurs haute résolution tout en conservant une précision statistique acceptable pour l'estimateur MAD. La conformité aux contrats de performance est validée sur cet aspect.

   ### 2. Points Faibles ou Goulots Restants
   - **Bogue critique de correction (`compute_mad_sigma`)** : La fonction `median` utilise `select_nth_unstable_by`, qui réordonne le slice en place. Or, les écarts absolus sont calculés *après* cette mutation (`values.iter().map(...)`). Les valeurs de `values` ayant été déplacées, la formule MAD (`|x_i - median|`) est appliquée sur un tableau périmé, produisant un profil de bruit statistiquement invalide. Il faut calculer les écarts avant le tri ou cloner les données.
   - **Allocation secondaire dans le chemin chaud** : `compute_mad_sigma` alloue un second `Vec<f32>` pour `deviations`. Bien que plafonnée, cette allocation supplémentaire ajoute de la pression sur le GC (bien que Rust n'en ait pas, c'est de la gestion de heap) et peut être évitée par un buffer réutilisable ou un calcul en deux passes sans allocation intermédiaire.
   - **Complexité du `median` pair** : Le calcul de la médiane pour un slice de longueur paire itère sur `slice[..mid]` pour trouver le maximum. C'est fonctionnel mais ajoute une passe O(n) supplémentaire. Un `select_nth_unstable` standard suivi d'un accès direct à `mid-1` serait plus idiomatique si le slice était trié, mais ici la partition suffit. À surveiller si la précision devient critique.
   - **Chemin EXIF** : Bien que sécurisé, l'enchaînement de `if let` et de `format!` reste séquentiel. Pour des flux de traitement parallèle massif, un parser basé sur `nom` ou `chumsky` pourrait réduire la surcharge, mais ce n'est pas un goulot actuel.

   ### 3. Note Révisée (/10) et Recommandation Finale
   **Note : 7.5/10**
   Les gains de pré-allocation sont solides et répondent au cahier des charges. Cependant, le bogue de mutation dans `compute_mad_sigma` invalide la fiabilité du fallback bruit, ce qui est inacceptable pour un module de traitement d'image.
   **Recommandations :**
   1. Corriger immédiatement `compute_mad_sigma` : calculer `deviations` à partir d'une copie ou avant l'appel à `median(values)`.
   2. Remplacer l'allocation de `deviations` par un `Vec<f32>` réutilisable passé par mutabilité ou un tableau fixe si `pts_cap` est connu statiquement.
   3. Valider les sorties du fallback MAD contre des profils de bruit connus avant de fusionner.
   Le code est prêt pour la production une fois la correction statistique appliquée.

---
