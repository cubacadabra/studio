use super::*;
pub(crate) fn configure_context(context: &egui::Context) {
    configure_fonts(context);
    configure_style(context);
}

pub(crate) fn configure_fonts(context: &egui::Context) {
    // egui rasterizes fonts itself, so use each desktop OS's installed UI face
    // and retain its bundled fonts as fallbacks for missing glyphs or files.
    let mut fonts = FontDefinitions::default();
    if install_font_face(
        &mut fonts,
        SYSTEM_UI_REGULAR,
        REGULAR_FONT_PATHS,
        REGULAR_FONT_WEIGHT,
    ) {
        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .expect("egui should define its proportional fallback family")
            .insert(0, SYSTEM_UI_REGULAR.to_owned());
    }
    let proportional = fonts
        .families
        .get(&FontFamily::Proportional)
        .expect("egui should define its proportional fallback family")
        .clone();

    let mut medium_family = Vec::new();
    if install_font_face(
        &mut fonts,
        SYSTEM_UI_MEDIUM,
        MEDIUM_FONT_PATHS,
        MEDIUM_FONT_WEIGHT,
    ) {
        medium_family.push(SYSTEM_UI_MEDIUM.to_owned());
    }
    medium_family.extend(proportional.iter().cloned());
    fonts
        .families
        .insert(FontFamily::Name(MEDIUM_FONT_FAMILY.into()), medium_family);

    let mut semibold_family = Vec::new();
    if install_font_face(
        &mut fonts,
        SYSTEM_UI_SEMIBOLD,
        SEMIBOLD_FONT_PATHS,
        SEMIBOLD_FONT_WEIGHT,
    ) {
        semibold_family.push(SYSTEM_UI_SEMIBOLD.to_owned());
    }
    semibold_family.extend(proportional);
    fonts.families.insert(
        FontFamily::Name(SEMIBOLD_FONT_FAMILY.into()),
        semibold_family,
    );
    context.set_fonts(fonts);
}

pub(crate) fn install_font_face(
    fonts: &mut FontDefinitions,
    name: &str,
    paths: &[&str],
    weight: f32,
) -> bool {
    let Some(bytes) = read_first_font(paths) else {
        return false;
    };
    fonts
        .font_data
        .insert(name.to_owned(), Arc::new(platform_font_data(bytes, weight)));
    true
}

pub(crate) fn platform_font_data(bytes: Vec<u8>, weight: f32) -> FontData {
    let data = FontData::from_owned(bytes);
    #[cfg(target_os = "macos")]
    {
        // SFNS defaults to its narrower display cut outside AppKit. Select the
        // text optical size explicitly so small editor labels match native UI.
        let mut tweak = FontTweak::default();
        tweak.coords.push(b"opsz", UI_OPTICAL_SIZE);
        tweak.coords.push(b"wght", weight);
        tweak.hinting = Some(true);
        tweak.subpixel_binning = Some(false);
        data.tweak(tweak)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = weight;
        data
    }
}

pub(crate) fn read_first_font(paths: &[&str]) -> Option<Vec<u8>> {
    paths.iter().find_map(|path| fs::read(path).ok())
}

pub(crate) fn medium_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MEDIUM_FONT_FAMILY.into()))
}

pub(crate) fn semibold_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD_FONT_FAMILY.into()))
}

pub(crate) fn configure_style(context: &egui::Context) {
    configure_theme_style(context, egui::Theme::Dark, DARK_PALETTE);
    configure_theme_style(context, egui::Theme::Light, LIGHT_PALETTE);
    context.set_theme(egui::ThemePreference::System);
}

pub(crate) fn configure_theme_style(context: &egui::Context, theme: egui::Theme, palette: Palette) {
    let mut style = (*context.style_of(theme)).clone();
    style.spacing.item_spacing = egui::vec2(4.0, 1.0);
    style.spacing.button_padding = egui::vec2(6.0, 1.0);
    style.spacing.interact_size.y = CONTROL_HEIGHT;
    style.spacing.indent = 12.0;
    style.spacing.menu_margin = Margin::same(4);
    style.animation_time = 0.15;
    style.visuals.dark_mode = theme == egui::Theme::Dark;
    style.visuals.text_options.font_hinting = true;
    style.visuals.text_options.subpixel_binning = false;
    style.visuals.panel_fill = palette.panel;
    style.visuals.window_fill = palette.panel_raised;
    style.visuals.window_stroke = Stroke::new(1.0, palette.border);
    style.visuals.window_corner_radius = egui::CornerRadius::same(2);
    style.visuals.menu_corner_radius = egui::CornerRadius::same(3);
    style.visuals.extreme_bg_color = palette.surface_deep;
    style.visuals.text_edit_bg_color = Some(palette.field);
    style.visuals.faint_bg_color = palette.surface;
    style.visuals.indent_has_left_vline = false;
    style.visuals.selection.bg_fill = palette.selection;
    style.visuals.selection.stroke = Stroke::new(1.0, palette.accent);
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, palette.muted);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::NONE;
    style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(1);
    style.visuals.widgets.inactive.bg_fill = Color32::TRANSPARENT;
    style.visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    style.visuals.widgets.inactive.bg_stroke = Stroke::NONE;
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, palette.text);
    style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(1);
    style.visuals.widgets.hovered.bg_fill = palette.panel_raised;
    style.visuals.widgets.hovered.weak_bg_fill = palette.panel_raised;
    style.visuals.widgets.hovered.bg_stroke = Stroke::NONE;
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, palette.text);
    style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(1);
    style.visuals.widgets.active.bg_fill = palette.selection;
    style.visuals.widgets.active.weak_bg_fill = palette.selection;
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0, palette.accent);
    style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(1);
    style.visuals.widgets.open.bg_fill = palette.field;
    style.visuals.widgets.open.weak_bg_fill = palette.field;
    style.visuals.widgets.open.corner_radius = egui::CornerRadius::same(2);
    // Egui removes text padding for frameless buttons, including menu labels.
    // Keep the frame geometry and make inactive menu frames transparent instead.
    style.visuals.button_frame = true;
    style.visuals.slider_trailing_fill = true;
    style.visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(TYPE.secondary));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(TYPE.secondary));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(TYPE.meta));
    style
        .text_styles
        .insert(TextStyle::Monospace, FontId::monospace(TYPE.secondary));
    context.set_style_of(theme, style);
}

pub(crate) fn palette(ui: &egui::Ui) -> Palette {
    if ui.visuals().dark_mode {
        DARK_PALETTE
    } else {
        LIGHT_PALETTE
    }
}

pub(crate) fn editor_frame(fill: Color32) -> Frame {
    Frame::NONE.fill(fill).inner_margin(Margin::same(0))
}

pub(crate) fn load_logo_texture(context: &egui::Context) -> egui::TextureHandle {
    let image = image::load_from_memory(LOGO_BYTES)
        .expect("Cubacadabra Studio logo should be a valid image")
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    context.load_texture(
        "cubacadabra-studio-logo",
        color_image,
        egui::TextureOptions::LINEAR,
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn install_system_icon_textures(context: &egui::Context) {
    let textures = MACOS_SYSTEM_SYMBOLS
        .iter()
        .filter_map(|&(icon, symbol)| {
            let png = crate::macos::system_symbol_png(symbol)?;
            let image = system_icon_color_image(&png)?;
            let texture = context.load_texture(
                format!("sf-symbol-{symbol}"),
                image,
                egui::TextureOptions::LINEAR,
            );
            Some((icon, texture))
        })
        .collect::<HashMap<_, _>>();
    context.data_mut(|data| {
        data.insert_temp(
            egui::Id::new(SYSTEM_ICON_ATLAS_ID),
            Arc::new(textures) as SystemIconAtlas,
        );
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn system_icon_color_image(png: &[u8]) -> Option<egui::ColorImage> {
    let rgba = image::load_from_memory(png).ok()?.to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut bounds = None::<(u32, u32, u32, u32)>;
    for (x, y, pixel) in rgba.enumerate_pixels() {
        if pixel[3] == 0 {
            continue;
        }
        bounds = Some(match bounds {
            Some((min_x, min_y, max_x, max_y)) => {
                (min_x.min(x), min_y.min(y), max_x.max(x), max_y.max(y))
            }
            None => (x, y, x, y),
        });
    }
    let (min_x, min_y, max_x, max_y) = bounds?;
    let min_x = min_x.saturating_sub(1);
    let min_y = min_y.saturating_sub(1);
    let max_x = (max_x + 1).min(width - 1);
    let max_y = (max_y + 1).min(height - 1);
    let mut cropped =
        image::imageops::crop_imm(&rgba, min_x, min_y, max_x - min_x + 1, max_y - min_y + 1)
            .to_image();
    for pixel in cropped.pixels_mut() {
        *pixel = image::Rgba([255, 255, 255, pixel[3]]);
    }
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [cropped.width() as usize, cropped.height() as usize],
        cropped.as_raw(),
    ))
}
