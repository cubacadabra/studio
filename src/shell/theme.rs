use super::*;
pub(crate) struct UiMetrics {
    pub(crate) top_bar: f32,
    pub(crate) status_bar: f32,
    pub(crate) editor_header: f32,
    pub(crate) control: f32,
    pub(crate) inset: f32,
    pub(crate) icon: f32,
    pub(crate) row: f32,
    pub(crate) radius: f32,
}

pub(crate) struct TypographyMetrics {
    pub(crate) primary: f32,
    pub(crate) secondary: f32,
    pub(crate) meta: f32,
}

pub(crate) const UI: UiMetrics = UiMetrics {
    top_bar: 30.0,
    status_bar: 20.0,
    editor_header: 24.0,
    control: 20.0,
    inset: 6.0,
    icon: 13.0,
    row: 22.0,
    radius: 1.0,
};
pub(crate) const TYPE: TypographyMetrics = TypographyMetrics {
    primary: 13.0,
    secondary: 12.0,
    meta: 11.0,
};
pub(crate) const MEDIUM_FONT_FAMILY: &str = "studio-system-ui-medium";
pub(crate) const SEMIBOLD_FONT_FAMILY: &str = "studio-system-ui-semibold";
pub(crate) const SYSTEM_UI_REGULAR: &str = "studio-system-ui-regular";
pub(crate) const SYSTEM_UI_MEDIUM: &str = "studio-system-ui-medium-face";
pub(crate) const SYSTEM_UI_SEMIBOLD: &str = "studio-system-ui-semibold-face";
pub(crate) const REGULAR_FONT_WEIGHT: f32 = 400.0;
pub(crate) const MEDIUM_FONT_WEIGHT: f32 = 510.0;
pub(crate) const SEMIBOLD_FONT_WEIGHT: f32 = 590.0;
#[cfg(target_os = "macos")]
pub(crate) const UI_OPTICAL_SIZE: f32 = 13.0;

#[cfg(target_os = "macos")]
pub(crate) const REGULAR_FONT_PATHS: &[&str] = &[
    "/System/Library/Fonts/SFNS.ttf",
    "/Library/Fonts/SF-Pro-Text-Regular.otf",
];
#[cfg(target_os = "macos")]
pub(crate) const MEDIUM_FONT_PATHS: &[&str] = &[
    "/System/Library/Fonts/SFNS.ttf",
    "/Library/Fonts/SF-Pro-Text-Medium.otf",
];
#[cfg(target_os = "macos")]
pub(crate) const SEMIBOLD_FONT_PATHS: &[&str] = &[
    "/System/Library/Fonts/SFNS.ttf",
    "/Library/Fonts/SF-Pro-Text-Semibold.otf",
];

#[cfg(target_os = "windows")]
pub(crate) const REGULAR_FONT_PATHS: &[&str] = &["C:/Windows/Fonts/segoeui.ttf"];
#[cfg(target_os = "windows")]
pub(crate) const MEDIUM_FONT_PATHS: &[&str] = &[
    "C:/Windows/Fonts/seguisb.ttf",
    "C:/Windows/Fonts/segoeuisl.ttf",
];
#[cfg(target_os = "windows")]
pub(crate) const SEMIBOLD_FONT_PATHS: &[&str] = &["C:/Windows/Fonts/seguisb.ttf"];

#[cfg(target_os = "linux")]
pub(crate) const REGULAR_FONT_PATHS: &[&str] = &[
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
];
#[cfg(target_os = "linux")]
pub(crate) const MEDIUM_FONT_PATHS: &[&str] = &[
    "/usr/share/fonts/truetype/noto/NotoSans-Medium.ttf",
    "/usr/share/fonts/truetype/liberation2/LiberationSans-Bold.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
];
#[cfg(target_os = "linux")]
pub(crate) const SEMIBOLD_FONT_PATHS: &[&str] = &[
    "/usr/share/fonts/truetype/noto/NotoSans-SemiBold.ttf",
    "/usr/share/fonts/truetype/liberation2/LiberationSans-Bold.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
];

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub(crate) const REGULAR_FONT_PATHS: &[&str] = &[];
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub(crate) const MEDIUM_FONT_PATHS: &[&str] = &[];
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub(crate) const SEMIBOLD_FONT_PATHS: &[&str] = &[];
pub(crate) const TOP_BAR_HEIGHT: f32 = UI.top_bar;
pub(crate) const STATUS_BAR_HEIGHT: f32 = UI.status_bar;
pub(crate) const EDITOR_HEADER_HEIGHT: f32 = UI.editor_header;
pub(crate) const CONTROL_HEIGHT: f32 = UI.control;
pub(crate) const LABEL_PADDING: f32 = UI.inset;
#[derive(Clone, Copy)]
pub(crate) struct Palette {
    pub(crate) panel: Color32,
    pub(crate) panel_raised: Color32,
    pub(crate) panel_header: Color32,
    pub(crate) surface: Color32,
    pub(crate) surface_deep: Color32,
    pub(crate) field: Color32,
    pub(crate) border: Color32,
    pub(crate) border_strong: Color32,
    pub(crate) text: Color32,
    pub(crate) muted: Color32,
    pub(crate) secondary_text: Color32,
    pub(crate) faint: Color32,
    pub(crate) accent: Color32,
    pub(crate) selection: Color32,
    pub(crate) asset_selection: Color32,
    pub(crate) asset_selection_stroke: Color32,
    pub(crate) live: Color32,
    pub(crate) axis_x: Color32,
}

pub(crate) const DARK_PALETTE: Palette = Palette {
    panel: Color32::from_rgb(30, 30, 30),
    panel_raised: Color32::from_rgb(35, 35, 35),
    panel_header: Color32::from_rgb(39, 39, 39),
    surface: Color32::from_rgb(27, 27, 27),
    surface_deep: Color32::from_rgb(23, 23, 23),
    field: Color32::from_rgb(47, 47, 47),
    border: Color32::from_rgb(48, 48, 48),
    border_strong: Color32::from_rgb(66, 66, 66),
    text: Color32::from_rgb(226, 226, 226),
    muted: Color32::from_rgb(170, 170, 170),
    secondary_text: Color32::from_rgb(200, 200, 200),
    faint: Color32::from_rgb(124, 124, 124),
    accent: Color32::from_rgb(122, 157, 193),
    selection: Color32::from_rgb(63, 78, 94),
    asset_selection: Color32::from_rgb(43, 48, 54),
    asset_selection_stroke: Color32::from_rgb(78, 96, 114),
    live: Color32::from_rgb(120, 166, 137),
    axis_x: Color32::from_rgb(218, 105, 105),
};

pub(crate) const LIGHT_PALETTE: Palette = Palette {
    panel: Color32::from_rgb(242, 242, 242),
    panel_raised: Color32::from_rgb(232, 232, 232),
    panel_header: Color32::from_rgb(224, 224, 224),
    surface: Color32::from_rgb(250, 250, 250),
    surface_deep: Color32::from_rgb(255, 255, 255),
    field: Color32::from_rgb(255, 255, 255),
    border: Color32::from_rgb(198, 198, 198),
    border_strong: Color32::from_rgb(166, 166, 166),
    text: Color32::from_rgb(35, 35, 35),
    muted: Color32::from_rgb(92, 92, 92),
    secondary_text: Color32::from_rgb(65, 65, 65),
    faint: Color32::from_rgb(128, 128, 128),
    accent: Color32::from_rgb(53, 103, 150),
    selection: Color32::from_rgb(204, 218, 232),
    asset_selection: Color32::from_rgb(222, 231, 240),
    asset_selection_stroke: Color32::from_rgb(118, 145, 171),
    live: Color32::from_rgb(54, 128, 79),
    axis_x: Color32::from_rgb(180, 55, 55),
};
pub(crate) const LOGO_BYTES: &[u8] = include_bytes!("../../assets/logo.png");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Icon {
    World,
    Assets,
    Material,
    Test,
    Folder,
    Object,
    Image,
    Character,
    Sparkles,
    Search,
    Filter,
    Grid,
    Camera,
    Sliders,
    More,
    Plus,
    Play,
    Stop,
    Check,
    ChevronDown,
    ChevronRight,
    Eye,
    Lock,
    Save,
    Open,
    Copy,
    #[cfg(not(target_os = "macos"))]
    Undo,
    #[cfg(not(target_os = "macos"))]
    Redo,
    #[cfg(not(target_os = "macos"))]
    Settings,
    Network,
    Logs,
    Gauge,
}

#[cfg(target_os = "macos")]
pub(crate) const MACOS_SYSTEM_SYMBOLS: &[(Icon, &str)] = &[
    (Icon::World, "globe"),
    (Icon::Assets, "square.grid.2x2"),
    (Icon::Material, "circle.lefthalf.filled"),
    (Icon::Test, "play.rectangle"),
    (Icon::Folder, "folder.fill"),
    (Icon::Object, "cube"),
    (Icon::Image, "photo"),
    (Icon::Character, "person"),
    (Icon::Sparkles, "sparkles"),
    (Icon::Search, "magnifyingglass"),
    (Icon::Filter, "line.3.horizontal.decrease"),
    (Icon::Grid, "square.grid.2x2"),
    (Icon::Camera, "camera"),
    (Icon::Sliders, "slider.horizontal.3"),
    (Icon::More, "ellipsis"),
    (Icon::Plus, "plus"),
    (Icon::Play, "play.fill"),
    (Icon::Stop, "stop.fill"),
    (Icon::Check, "checkmark"),
    (Icon::ChevronDown, "chevron.down"),
    (Icon::ChevronRight, "chevron.right"),
    (Icon::Eye, "eye"),
    (Icon::Lock, "lock"),
    (Icon::Open, "square.and.arrow.down"),
    (Icon::Save, "square.and.arrow.down.fill"),
    (Icon::Copy, "doc.on.doc"),
    (Icon::Network, "network"),
    (Icon::Logs, "list.bullet.rectangle"),
    (Icon::Gauge, "speedometer"),
];

#[cfg(target_os = "macos")]
pub(crate) const SYSTEM_ICON_ATLAS_ID: &str = "studio-macos-system-icons";

#[cfg(target_os = "macos")]
pub(crate) type SystemIconAtlas = Arc<HashMap<Icon, egui::TextureHandle>>;
