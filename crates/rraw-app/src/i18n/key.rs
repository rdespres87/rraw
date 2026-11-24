#![forbid(unsafe_code)]

//! Strongly-typed internationalization translation keys.
//!
//! Declared via a single-source-of-truth declarative macro so `Key`, `ALL_KEYS`,
//! `Key::COUNT`, `Key::as_str`, and `core::str::FromStr` can never drift out of sync.

use core::fmt;
use core::str::FromStr;

use super::{I18nEngine, Language};

/// Error returned when parsing an unknown translation key identifier via [`FromStr`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseKeyError;

impl fmt::Display for ParseKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unknown i18n translation key")
    }
}

impl std::error::Error for ParseKeyError {}

macro_rules! define_i18n_keys {
    ($($variant:ident),+ $(,)?) => {
        /// Enumeration of all localized strings across the Rraw interface.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(usize)]
        pub enum Key {
            $($variant,)+
        }

        impl Key {
            /// Total number of strongly-typed translation keys.
            pub const COUNT: usize = [$(Self::$variant),+].len();

            /// Returns the 0-based dense index corresponding to this key.
            #[inline]
            #[must_use]
            pub const fn index(self) -> usize {
                self as usize
            }

            /// Reconstructs a `Key` from its dense 0-based index.
            #[inline]
            #[must_use]
            pub const fn from_index(index: usize) -> Option<Self> {
                if index < Self::COUNT {
                    Some(ALL_KEYS[index])
                } else {
                    None
                }
            }

            /// Returns the string slice identifier matching this key in JSON catalogs.
            #[inline]
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => stringify!($variant),)+
                }
            }

            /// Parses a string slice into a strongly-typed `Key`.
            #[inline]
            #[must_use]
            pub fn from_str_name(s: &str) -> Option<Self> {
                s.parse().ok()
            }

            /// Translates this key using the global `I18nEngine`.
            #[inline]
            #[must_use]
            pub fn translate(self, lang: Language) -> &'static str {
                I18nEngine::global().translate(lang, self)
            }
        }

        impl FromStr for Key {
            type Err = ParseKeyError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $(stringify!($variant) => Ok(Self::$variant),)+
                    _ => Err(ParseKeyError),
                }
            }
        }

        /// Dense ordered slice of all strongly-typed translation keys.
        pub const ALL_KEYS: [Key; Key::COUNT] = [
            $(Key::$variant,)+
        ];
    };
}

define_i18n_keys!(
    AboutBuildDate,
    AboutClose,
    AboutCommit,
    AboutDescription,
    AboutTitle,
    AboutVersion,
    CropHint,
    CropOverlayBanner,
    CropOverlayValidate,
    CropRatio16x9,
    CropRatio3x2,
    CropRatio4x3,
    CropRatio5x4,
    CropRatio9x16,
    CropRatioFree,
    CropRatioOriginal,
    CropRatioSquare,
    CurveChannelBlue,
    CurveChannelGreen,
    CurveChannelLuma,
    CurveChannelRed,
    CurveChannelRgb,
    DarkroomActiveOnly,
    DarkroomActiveOnlyActive,
    DarkroomActiveOnlyTooltip,
    DarkroomResetLayout,
    DarkroomResetLayoutTooltip,
    DarkroomTitle,
    DetailAneBadgeActive,
    DetailAneBadgeFallback,
    DetailAneDenoise,
    DetailAneDenoiseTooltip,
    DetailAneStatusTile,
    DetailAneStrength,
    DetailAutoDenoise,
    DetailAutoDenoiseTooltip,
    DetailChromaTooltip,
    DetailLumaTooltip,
    ExifAllTagsDetailed,
    ExifAperture,
    ExifCamera,
    ExifCopyButton,
    ExifDate,
    ExifDimensions,
    ExifExportButton,
    ExifFilterTags,
    ExifFocalLength,
    ExifIso,
    ExifLens,
    ExifMake,
    ExifShutter,
    ExifTagAperture,
    ExifTagArtist,
    ExifTagCameraMake,
    ExifTagCameraModel,
    ExifTagCameraOwner,
    ExifTagCameraSerialNumber,
    ExifTagColorSpace,
    ExifTagCopyright,
    ExifTagDateTimeOriginal,
    ExifTagDigitalZoomRatio,
    ExifTagExposureBias,
    ExifTagExposureMode,
    ExifTagExposureProgram,
    ExifTagExposureTime,
    ExifTagFlash,
    ExifTagFocalLength,
    ExifTagFocalLength35mmEq,
    ExifTagGpsAltitude,
    ExifTagGpsLatitude,
    ExifTagGpsLongitude,
    ExifTagIsoSensitivity,
    ExifTagLensMake,
    ExifTagLensModel,
    ExifTagLensSerialNumber,
    ExifTagLightSource,
    ExifTagMeteringMode,
    ExifTagOrientation,
    ExifTagSceneCaptureType,
    ExifTagSensingMethod,
    ExifTagShutterSpeed,
    ExifTagSoftware,
    ExifTagWhiteBalanceMode,
    ExifValAction,
    ExifValAperturePriority,
    ExifValAuto,
    ExifValAutoBracket,
    ExifValAverage,
    ExifValCenterWeighted,
    ExifValCloudyWeather,
    ExifValCreative,
    ExifValDaylight,
    ExifValFineWeather,
    ExifValFlash,
    ExifValFlashDidNotFire,
    ExifValFlashFired,
    ExifValFluorescent,
    ExifValLandscape,
    ExifValManual,
    ExifValMultiSpot,
    ExifValNightScene,
    ExifValNoFlash,
    ExifValOther,
    ExifValPartial,
    ExifValPattern,
    ExifValPortrait,
    ExifValProgramAe,
    ExifValShade,
    ExifValShutterPriority,
    ExifValSpot,
    ExifValStandard,
    ExifValTungsten,
    ExifValUnknown,
    ExportActivePhoto,
    ExportBitDepth,
    ExportBitDepth16Lossless,
    ExportBitDepth16Studio,
    ExportBitDepth8,
    ExportBitDepth8Standard,
    ExportBtnBatch,
    ExportBtnSingle,
    ExportBtnTimelapse,
    ExportButton,
    ExportCancel,
    ExportCropApplied,
    ExportCustomHeight,
    ExportCustomWidth,
    ExportDestinationHeading,
    ExportFailedPrefix,
    ExportFileSuffix,
    ExportFormatHeading,
    ExportJpegQuality,
    ExportJpegQualityBalanced,
    ExportJpegQualityLow,
    ExportJpegQualityMax,
    ExportNoBatchWarning,
    ExportResizeHeading,
    ExportScale2048,
    ExportScaleCustom,
    ExportScaleFull,
    ExportScaleHalf,
    ExportScaleOriginal,
    ExportScaleQuarter,
    ExportSensorResolution,
    ExportSequencePrefix,
    ExportSourceFolder,
    ExportSuccessBatch,
    ExportSuccessSinglePrefix,
    ExportSuccessTimelapse,
    ExportTargetBatch,
    ExportTargetCurrent,
    ExportTargetHeading,
    ExportTargetTimelapse,
    ExportTitle,
    ExportUseSubfolder,
    ExportWindowTitle,
    FamilyAll,
    FamilyAllShort,
    FamilyColor,
    FamilyColorShort,
    FamilyDetail,
    FamilyDetailShort,
    FamilyGeometry,
    FamilyGeometryShort,
    FamilyLight,
    FamilyLightShort,
    FamilyLocalMasks,
    FamilyLocalMasksShort,
    FamilyTimelapse,
    FamilyTimelapseShort,
    FilmstripCopyRecipe,
    FilmstripDeselectAll,
    FilmstripNoPhotos,
    FilmstripPasteRecipeSelection,
    FilmstripPasteRecipeThis,
    FilmstripSelectAll,
    FilmstripSelectAllTooltip,
    FilmstripSelectFolderPrompt,
    FilmstripToggleExport,
    FilmstripToolbarCopy,
    FilmstripToolbarCopyTooltip,
    FilmstripToolbarPaste,
    FilmstripToolbarPasteSelection,
    GeometryAdjustedStatus,
    GeometryLine1Label,
    GeometryParallelBtn,
    GeometryParallelBtnActive,
    GeometryParallelStatus1,
    GeometryParallelStatus2,
    HistogramHighlightClippingTooltip,
    HistogramShadowClippingTooltip,
    InspectorAddBrushTooltip,
    InspectorAddLinearTooltip,
    InspectorAddRadialTooltip,
    InspectorAdjustCrop,
    InspectorAiDeleteTooltip,
    InspectorAiInvertTooltip,
    InspectorAiMasksSubtitle,
    InspectorAiMasksTitle,
    InspectorAiOverlayTooltip,
    InspectorAutoCrop,
    InspectorBrushRadiusTooltip,
    InspectorCenterX,
    InspectorCenterY,
    InspectorCorrectionStrength,
    InspectorCreativeVignetteTitle,
    InspectorDefringePreviewMask,
    InspectorDefringePreviewMaskTooltip,
    InspectorDeleteMaskTooltip,
    InspectorDrtAgx,
    InspectorDrtOpenDrt,
    InspectorEnableCrop,
    InspectorEnableGeometry,
    InspectorEnableSoftProof,
    InspectorEndY,
    InspectorFeather,
    InspectorGamutAlert,
    InspectorGamutAlertStrong,
    InspectorGamutAlertTooltip,
    InspectorGeometricMasksTitle,
    InspectorInvertTooltip,
    InspectorLensCa,
    InspectorLensDistortion,
    InspectorLensManualTitle,
    InspectorLensProfileAuto,
    InspectorLensProfileAutoTooltip,
    InspectorLensProfileCa,
    InspectorLensProfileCalibratedBadge,
    InspectorLensProfileDetected,
    InspectorLensProfileDisabledNotice,
    InspectorLensProfileDistortion,
    InspectorLensProfileDngBadge,
    InspectorLensProfileGeneric,
    InspectorLensProfileManualBadge,
    InspectorLensProfileNoMatch,
    InspectorLensProfileReset,
    InspectorLensProfileSearch,
    InspectorLensProfileSearchHint,
    InspectorLensProfileTitle,
    InspectorLensProfileVignette,
    InspectorLensTabManual,
    InspectorLensTabProfile,
    InspectorLensVignette,
    InspectorLensVignetteTitle,
    InspectorMoveDown,
    InspectorMoveUp,
    InspectorNoMatchingTools,
    InspectorOverlayTooltip,
    InspectorPaintButton,
    InspectorPaintTooltip,
    InspectorParamAddBrush,
    InspectorParamAddLinear,
    InspectorParamAddRadial,
    InspectorParamBlacks,
    InspectorParamBrushFeather,
    InspectorParamBrushFlow,
    InspectorParamBrushRadius,
    InspectorParamCaptureSharpening,
    InspectorParamChannel,
    InspectorParamChannelBlue,
    InspectorParamChannelGreen,
    InspectorParamChannelLuma,
    InspectorParamChannelRed,
    InspectorParamChannelRgb,
    InspectorParamChromaDenoise,
    InspectorParamContrast,
    InspectorParamCropApply,
    InspectorParamCropLock,
    InspectorParamCropRatio,
    InspectorParamDefringeGreen,
    InspectorParamDefringePurple,
    InspectorParamDefringeThreshold,
    InspectorParamDistortion,
    InspectorParamDrt,
    InspectorParamExposure,
    InspectorParamFlipH,
    InspectorParamFlipV,
    InspectorParamGeometricMasks,
    InspectorParamHighlights,
    InspectorParamInvertMask,
    InspectorParamKelvin,
    InspectorParamKeystoneH,
    InspectorParamKeystoneV,
    InspectorParamLumaDenoise,
    InspectorParamMaskOpacity,
    InspectorParamMonochrome,
    InspectorParamSaturation,
    InspectorParamSemanticMasks,
    InspectorParamShadows,
    InspectorParamSoftProofGamutAlert,
    InspectorParamSoftProofIntent,
    InspectorParamSoftProofPaper,
    InspectorParamSoftProofProfile,
    InspectorParamStraightenAngle,
    InspectorParamTimelapseDeflicker,
    InspectorParamTimelapseRamping,
    InspectorParamTimelapseTitle,
    InspectorParamTint,
    InspectorParamVibrance,
    InspectorParamVignetteAmount,
    InspectorParamVignetteFeather,
    InspectorParamVignetteMidpoint,
    InspectorParamVignetteRoundness,
    InspectorParamWaveletSigma,
    InspectorRadiusX,
    InspectorRadiusY,
    InspectorResetAiMasks,
    InspectorResetAngle,
    InspectorResetColor,
    InspectorResetCrop,
    InspectorResetDetailDenoise,
    InspectorResetEffects,
    InspectorResetGeometry,
    InspectorResetOptical,
    InspectorResetSoftProof,
    InspectorResetTone,
    InspectorRotate90Left,
    InspectorRotate90Right,
    InspectorShowAllTools,
    InspectorShowCropGrid,
    InspectorSmartLightingHighlightComp,
    InspectorSmartLightingIntensity,
    InspectorSmartLightingModeCustom,
    InspectorSmartLightingModeMedium,
    InspectorSmartLightingModeOff,
    InspectorSmartLightingModeSlight,
    InspectorSmartLightingModeStrong,
    InspectorSmartLightingShadowLift,
    InspectorSmartLightingTitle,
    InspectorSmartLightingTooltip,
    InspectorSmoothness,
    InspectorStartY,
    InspectorTimelapseExport,
    InspectorValidateCrop,
    MaskAiDeselectTooltip,
    MaskAiHardwareAne,
    MaskAiSelectTooltip,
    MaskBrushClearTooltip,
    MaskBrushEraseTooltip,
    MaskBrushFlowTooltip,
    MaskBrushPaintTooltip,
    MaskBrushPrefix,
    MaskInvertLabel,
    MaskLinearPrefix,
    MaskRadialPrefix,
    PresetsImportLut,
    PresetsImportLutTooltip,
    PresetsLutIntensity,
    PresetsLutLabel,
    PresetsRemoveLutTooltip,
    PresetsResetTooltip,
    PresetsStyleLabel,
    ResetModuleTooltip,
    SectionAiSemanticMasks,
    SectionCrop,
    SectionDetailDenoise,
    SectionEffects,
    SectionExposureTone,
    SectionGeometry,
    SectionOpticalCorrections,
    SectionSoftProofing,
    SectionStraighten,
    SectionTimelapse,
    SectionToneCurves,
    SectionWhiteBalanceColor,
    SemanticMaskBackground,
    SemanticMaskPortrait,
    SemanticMaskSky,
    SemanticMaskSubject,
    ShortcutsClose,
    ShortcutsSectionBatch,
    ShortcutsSectionBrush,
    ShortcutsSectionFiles,
    ShortcutsSectionNav,
    ShortcutsSectionView,
    ShortcutsSubtitle,
    ShortcutsTitle,
    ShortcutsWindowTitle,
    SidebarBrowseButton,
    SidebarBrowseTooltip,
    SidebarExifTitle,
    SidebarExplorerTitle,
    SidebarHistogramTitle,
    SidebarNoPhotoLoaded,
    SidebarPhotosCount,
    SidebarPhotosCountPlural,
    SidebarPresetsTitle,
    StatusNoPhoto,
    StatusSurfaceInfo,
    StraightenHorizonActive,
    StraightenHorizonDraw,
    StraightenHorizonStatus,
    TimelapseDeflickerTitle,
    TimelapseDeflickerTooltip,
    TimelapseDuration,
    TimelapseEndLabel,
    TimelapseExpShort,
    TimelapseExpTooltip,
    TimelapseFps,
    TimelapseFramesCount,
    TimelapseHolyGrailTitle,
    TimelapseHolyGrailTooltip,
    TimelapseImage1,
    TimelapseImageN,
    TimelapseIntensity,
    TimelapseStartLabel,
    TimelapseTempShort,
    TimelapseTempTooltip,
    TimelapseViewButton,
    TimelapseViewEndTooltip,
    TimelapseViewStartTooltip,
    TimelapseWindowSize,
    TimelapseWindowSuffix,
    ToggleBeforeAfter,
    ToggleBeforeAfterTooltip,
    ToggleClipping,
    ToggleClippingTooltip,
    ToggleFilmstrip,
    ToggleFilmstripTooltip,
    ToggleInfoHud,
    ToggleInfoHudTooltip,
    ToggleShortcuts,
    ToggleShortcutsTooltip,
    ToneCurvesHoverPrompt,
    ToneCurvesResetAll,
    ToneCurvesResetAllTooltip,
    TopBarAbout,
    TopBarCenter,
    TopBarCenterTooltip,
    TopBarEdrHeadroom,
    TopBarEdrTooltip,
    TopBarExport,
    TopBarGamutAlert,
    TopBarGamutOk,
    TopBarGamutTooltip,
    TopBarOpenFolder,
    TopBarOpenImage,
    TopBarPaperInk,
    TopBarPaperInkTooltip,
    TopBarSdrHeadroom,
    TopBarSdrTooltip,
    TopBarSoftProofActive,
    TopBarSoftProofInactive,
    TopBarSoftProofTooltip,
    TopBarTimelapse,
    TopBarTimelapseTooltip,
    TopBarZoom,
    TopBarZoom11,
    TopBarZoomFit,
    TopBarZoomFitted,
    ViewportAfter,
    ViewportBefore,
    ViewportOriginalBadge,
    EscapeCropDeactivated,
    EscapeHorizonDeactivated,
    EscapePerspectiveDeactivated,
    EscapeBrushDeactivated,
    EscapeSelectionReset,
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_str_round_trips_all_keys_and_rejects_unknown() {
        assert_eq!(ALL_KEYS.len(), Key::COUNT);
        for (idx, &key) in ALL_KEYS.iter().enumerate() {
            assert_eq!(key.index(), idx);
            assert_eq!(Key::from_index(idx), Some(key));
            let parsed: Result<Key, ParseKeyError> = key.as_str().parse();
            assert_eq!(parsed, Ok(key));
            assert_eq!(Key::from_str_name(key.as_str()), Some(key));
        }
        assert_eq!(Key::from_index(Key::COUNT), None);

        let err = "NonExistentTranslationKey".parse::<Key>();
        assert_eq!(err, Err(ParseKeyError));
        if let Err(parse_err) = err {
            assert!(!parse_err.to_string().is_empty());
        }
    }
}
