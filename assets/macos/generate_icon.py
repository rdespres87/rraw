#!/usr/bin/env python3
"""
Generate high-fidelity macOS application icon and iconset for Rraw.
Produces master 1024x1024 icon and Apple ICNS bundle (assets/macos/AppIcon.icns).
"""

import math
import os
import shutil
import subprocess
import sys
from PIL import Image, ImageDraw, ImageFilter


def create_master_icon(size: int = 1024) -> Image.Image:
    # Render at 2x resolution for ultra-sharp supersampled antialiasing
    scale = 2
    canvas_size = size * scale
    img = Image.new("RGBA", (canvas_size, canvas_size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    # macOS squircle parameters (standard squircle rounded rect in 824x824 bounding box inside 1024)
    # Scaled to canvas_size
    margin = int(100 * scale)
    box_w = canvas_size - 2 * margin
    radius = int(185 * scale)
    box = [margin, margin, margin + box_w, margin + box_w]

    # 1. Background Mask & Shadow
    shadow_offset = int(16 * scale)
    shadow_blur = int(24 * scale)
    shadow_layer = Image.new("RGBA", (canvas_size, canvas_size), (0, 0, 0, 0))
    shadow_draw = ImageDraw.Draw(shadow_layer)
    shadow_box = [margin, margin + shadow_offset, margin + box_w, margin + box_w + shadow_offset]
    shadow_draw.rounded_rectangle(shadow_box, radius=radius, fill=(0, 0, 0, 140))
    shadow_layer = shadow_layer.filter(ImageFilter.GaussianBlur(shadow_blur))
    img.alpha_composite(shadow_layer)

    # 2. Main squircle base with photographic darkroom gradient
    # Deep obsidian (#10131a -> #090a0e)
    base_layer = Image.new("RGBA", (canvas_size, canvas_size), (0, 0, 0, 0))
    base_draw = ImageDraw.Draw(base_layer)
    base_draw.rounded_rectangle(box, radius=radius, fill=(16, 19, 26, 255))

    # Apply vertical gradient
    gradient = Image.new("RGBA", (canvas_size, canvas_size), (0, 0, 0, 0))
    for y in range(margin, margin + box_w):
        t = (y - margin) / float(box_w)
        r = int(26 * (1.0 - t) + 9 * t)
        g = int(32 * (1.0 - t) + 11 * t)
        b = int(46 * (1.0 - t) + 16 * t)
        line_draw = ImageDraw.Draw(gradient)
        line_draw.line([(margin, y), (margin + box_w, y)], fill=(r, g, b, 255))

    # Mask gradient to squircle
    mask = Image.new("L", (canvas_size, canvas_size), 0)
    mask_draw = ImageDraw.Draw(mask)
    mask_draw.rounded_rectangle(box, radius=radius, fill=255)
    base_layer = Image.composite(gradient, base_layer, mask)
    img.alpha_composite(base_layer)

    # 3. Subtle Metallic Rim Border (#384259)
    border_layer = Image.new("RGBA", (canvas_size, canvas_size), (0, 0, 0, 0))
    border_draw = ImageDraw.Draw(border_layer)
    border_width = int(4 * scale)
    border_draw.rounded_rectangle(box, radius=radius, outline=(56, 66, 89, 220), width=border_width)

    # Top highlight rim
    highlight_box = [margin + border_width, margin + border_width, margin + box_w - border_width, margin + int(30 * scale)]
    border_draw.line([(margin + radius // 2, margin + border_width), (margin + box_w - radius // 2, margin + border_width)], fill=(120, 140, 180, 160), width=int(2 * scale))
    img.alpha_composite(border_layer)

    # 4. Center Camera Aperture / Darkroom Lens
    cx = canvas_size // 2
    cy = canvas_size // 2
    outer_lens_r = int(260 * scale)
    inner_barrel_r = int(240 * scale)

    # Outer lens ring
    lens_layer = Image.new("RGBA", (canvas_size, canvas_size), (0, 0, 0, 0))
    lens_draw = ImageDraw.Draw(lens_layer)
    lens_draw.ellipse([cx - outer_lens_r, cy - outer_lens_r, cx + outer_lens_r, cy + outer_lens_r], fill=(22, 26, 36, 255), outline=(45, 54, 75, 255), width=int(3 * scale))
    # Inner knurled ring
    lens_draw.ellipse([cx - inner_barrel_r, cy - inner_barrel_r, cx + inner_barrel_r, cy + inner_barrel_r], fill=(12, 14, 20, 255), outline=(30, 36, 50, 255), width=int(2 * scale))

    # 5. HDR/EDR Light Core (Radial gradient simulating wide dynamic range light burst)
    light_layer = Image.new("RGBA", (canvas_size, canvas_size), (0, 0, 0, 0))
    light_draw = ImageDraw.Draw(light_layer)
    light_r = int(120 * scale)

    steps = 40
    for i in range(steps, 0, -1):
        t = i / float(steps)
        r_step = int(light_r * t)
        # Gradient: Core pure white -> bright amber/orange (#f59e0b) -> ACEScg magenta/violet (#8b5cf6) -> deep blue/cyan
        if t < 0.25:
            # White core
            r_c = 255
            g_c = 255
            b_c = 255
            alpha = int(255 * (1.0 - t * 0.5))
        elif t < 0.6:
            # Golden amber
            s = (t - 0.25) / 0.35
            r_c = int(255 * (1.0 - s) + 245 * s)
            g_c = int(255 * (1.0 - s) + 158 * s)
            b_c = int(255 * (1.0 - s) + 11 * s)
            alpha = int(230 * (1.0 - s * 0.3))
        else:
            # Violet / Deep Blue EDR trail
            s = (t - 0.6) / 0.4
            r_c = int(245 * (1.0 - s) + 99 * s)
            g_c = int(158 * (1.0 - s) + 102 * s)
            b_c = int(11 * (1.0 - s) + 241 * s)
            alpha = int(160 * (1.0 - s))
        light_draw.ellipse([cx - r_step, cy - r_step, cx + r_step, cy + r_step], fill=(r_c, g_c, b_c, alpha))

    # 6. Aperture Blades (6 titanium blades forming hexagonal aperture opening)
    blades_layer = Image.new("RGBA", (canvas_size, canvas_size), (0, 0, 0, 0))
    blades_draw = ImageDraw.Draw(blades_layer)
    num_blades = 6
    aperture_open_r = int(72 * scale)
    blade_outer_r = inner_barrel_r - int(6 * scale)

    for i in range(num_blades):
        angle_start = (i * 2.0 * math.pi) / num_blades
        angle_next = ((i + 1) * 2.0 * math.pi) / num_blades
        angle_mid = (angle_start + angle_next) / 2.0

        p0 = (cx + aperture_open_r * math.cos(angle_start), cy + aperture_open_r * math.sin(angle_start))
        p1 = (cx + aperture_open_r * math.cos(angle_next), cy + aperture_open_r * math.sin(angle_next))
        p2 = (cx + blade_outer_r * math.cos(angle_next + 0.3), cy + blade_outer_r * math.sin(angle_next + 0.3))
        p3 = (cx + blade_outer_r * math.cos(angle_start + 0.2), cy + blade_outer_r * math.sin(angle_start + 0.2))

        # Blade shading (titanium gray with subtle angle variation)
        blade_tone = 34 + int(12 * math.cos(angle_mid))
        blade_fill = (blade_tone, blade_tone + 3, blade_tone + 8, 255)
        blades_draw.polygon([p0, p1, p2, p3], fill=blade_fill, outline=(blade_tone + 20, blade_tone + 24, blade_tone + 32, 255))

    # Mask blades to inner barrel
    lens_mask = Image.new("L", (canvas_size, canvas_size), 0)
    lens_mask_draw = ImageDraw.Draw(lens_mask)
    lens_mask_draw.ellipse([cx - inner_barrel_r, cy - inner_barrel_r, cx + inner_barrel_r, cy + inner_barrel_r], fill=255)

    composite_lens = Image.new("RGBA", (canvas_size, canvas_size), (0, 0, 0, 0))
    composite_lens.alpha_composite(lens_layer)
    composite_lens.alpha_composite(light_layer)
    composite_lens.alpha_composite(blades_layer)

    img.paste(composite_lens, (0, 0), lens_mask)

    # 7. Stylized RAW photographic badge / lens index mark
    badge_draw = ImageDraw.Draw(img)
    # Index red pip at 12 o'clock
    pip_y = cy - outer_lens_r - int(12 * scale)
    badge_draw.ellipse([cx - int(4 * scale), pip_y - int(4 * scale), cx + int(4 * scale), pip_y + int(4 * scale)], fill=(239, 68, 68, 240))

    # Downsample from 2x (canvas_size) to target size with high-quality Lanczos resampling
    master = img.resize((size, size), Image.Resampling.LANCZOS)
    return master


def main():
    script_dir = os.path.dirname(os.path.abspath(__file__))
    iconset_dir = os.path.join(script_dir, "AppIcon.iconset")
    icns_path = os.path.join(script_dir, "AppIcon.icns")
    master_png_path = os.path.join(script_dir, "AppIcon_1024.png")

    print(f"==> Generating Rraw master 1024x1024 application icon...")
    master = create_master_icon(1024)
    master.save(master_png_path, "PNG")
    print(f"    Saved master icon: {master_png_path}")

    # Required iconset resolutions for macOS HiDPI
    resolutions = [
        ("icon_16x16.png", 16),
        ("icon_16x16@2x.png", 32),
        ("icon_32x32.png", 32),
        ("icon_32x32@2x.png", 64),
        ("icon_128x128.png", 128),
        ("icon_128x128@2x.png", 256),
        ("icon_256x256.png", 256),
        ("icon_256x256@2x.png", 512),
        ("icon_512x512.png", 512),
        ("icon_512x512@2x.png", 1024),
    ]

    os.makedirs(iconset_dir, exist_ok=True)
    for filename, px in resolutions:
        out_file = os.path.join(iconset_dir, filename)
        resized = master.resize((px, px), Image.Resampling.LANCZOS)
        resized.save(out_file, "PNG")

    print(f"    Generated {len(resolutions)} iconset images in {iconset_dir}")

    # Build .icns using macOS native iconutil
    if shutil.which("iconutil"):
        print(f"==> Compiling Apple ICNS bundle via iconutil...")
        subprocess.run(["iconutil", "-c", "icns", iconset_dir, "-o", icns_path], check=True)
        print(f"    Success: {icns_path} ({os.path.getsize(icns_path)} bytes)")
        # Clean up temporary iconset directory
        if "--keep-iconset" not in sys.argv:
            shutil.rmtree(iconset_dir, ignore_errors=True)
            print("    Cleaned temporary iconset directory.")
    else:
        print("Warning: iconutil not found. ICNS bundle was not generated.", file=sys.stderr)


if __name__ == "__main__":
    main()
