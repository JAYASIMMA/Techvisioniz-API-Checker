//! VS Code colour schemes (Default Dark+ / Default Light+) mapped onto egui.

use eframe::egui::{self, Color32, Context, Stroke, Visuals};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    Light,
    Dark,
    System,
}

impl Default for Theme {
    fn default() -> Self {
        Theme::System
    }
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::Light, Theme::Dark, Theme::System];

    pub fn label(self) -> &'static str {
        match self {
            Theme::Light => "Light (VS Code)",
            Theme::Dark => "Dark (VS Code)",
            Theme::System => "Match Windows",
        }
    }

    pub fn resolve(self, system: Option<egui::Theme>) -> bool {
        match self {
            Theme::Light => false,
            Theme::Dark => true,
            Theme::System => matches!(system, Some(egui::Theme::Dark)),
        }
    }
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb(((hex >> 16) & 0xFF) as u8, ((hex >> 8) & 0xFF) as u8, (hex & 0xFF) as u8)
}

/// VS Code surface colours, so panels and fields can be matched exactly.
#[derive(Clone, Copy)]
pub struct Surface {
    pub editor: Color32,
    pub panel: Color32,
    pub border: Color32,
    pub hover: Color32,
    pub active: Color32,
    pub selection: Color32,
    pub text: Color32,
    pub focus: Color32,
}

/// Accent colours used for statuses, HTTP verbs and messages.
#[derive(Clone, Copy)]
pub struct Palette {
    pub ok: Color32,
    pub warn: Color32,
    pub err: Color32,
    pub info: Color32,
    pub muted: Color32,
    pub get: Color32,
    pub post: Color32,
    pub put: Color32,
    pub patch: Color32,
    pub delete: Color32,
    pub surface: Surface,
}

impl Palette {
    /// Default Dark+ tokens.
    pub fn dark() -> Self {
        Self {
            ok: rgb(0x4E_C9_9A),
            warn: rgb(0xCC_A7_00),
            err: rgb(0xF4_87_71),
            info: rgb(0x37_94_FF),
            muted: rgb(0x9D_9D_9D),
            get: rgb(0x56_9C_D6),
            post: rgb(0x8F_BC_5D),
            put: rgb(0xC5_86_C0),
            patch: rgb(0x4E_C9_B0),
            delete: rgb(0xF4_87_71),
            surface: Surface {
                editor: rgb(0x1E_1E_1E),
                panel: rgb(0x25_25_26),
                border: rgb(0x3F_3F_46),
                hover: rgb(0x2A_2D_2E),
                active: rgb(0x09_47_71),
                selection: rgb(0x09_47_71),
                text: rgb(0xCC_CC_CC),
                focus: rgb(0x00_7F_D4),
            },
        }
    }

    /// Default Light+ tokens.
    pub fn light() -> Self {
        Self {
            ok: rgb(0x10_71_3E),
            // VS Code's own #BF8803 warning amber only reaches 2.8:1 on white, so
            // the text-capable variant is darkened to stay legible.
            warn: rgb(0x8A_61_00),
            err: rgb(0xC7_2E_0F),
            info: rgb(0x00_6A_B3),
            muted: rgb(0x61_61_61),
            get: rgb(0x0A_66_A8),
            post: rgb(0x2B_71_19),
            put: rgb(0x7A_3C_9E),
            patch: rgb(0x1B_6E_7F),
            delete: rgb(0xC7_2E_0F),
            surface: Surface {
                editor: rgb(0xFF_FF_FF),
                panel: rgb(0xF3_F3_F3),
                border: rgb(0xCE_CE_CE),
                hover: rgb(0xE5_F1_FB),
                active: rgb(0xCC_E4_F7),
                selection: rgb(0xAD_D6_FF),
                text: rgb(0x1F_1F_1F),
                focus: rgb(0x00_7F_D4),
            },
        }
    }

    pub fn for_dark(dark: bool) -> Self {
        if dark {
            Self::dark()
        } else {
            Self::light()
        }
    }

    pub fn status(&self, status: Option<u16>) -> Color32 {
        match status {
            Some(s) if (200..300).contains(&s) => self.ok,
            Some(s) if (300..400).contains(&s) => self.warn,
            Some(s) if (400..600).contains(&s) => self.err,
            Some(_) => self.muted,
            None => self.err,
        }
    }

    pub fn method(&self, method: &str) -> Color32 {
        match method {
            "GET" => self.get,
            "POST" => self.post,
            "PUT" => self.put,
            "PATCH" => self.patch,
            "DELETE" => self.delete,
            _ => self.muted,
        }
    }
}

pub fn visuals(dark: bool, palette: &Palette) -> Visuals {
    let s = palette.surface;
    let mut visuals = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };

    visuals.panel_fill = s.panel;
    visuals.window_fill = s.editor;
    visuals.extreme_bg_color = s.editor;
    visuals.faint_bg_color = if dark { rgb(0x2D_2D_30) } else { rgb(0xF8_F8_F8) };
    visuals.override_text_color = Some(s.text);
    visuals.hyperlink_color = palette.info;
    visuals.warn_fg_color = palette.warn;
    visuals.error_fg_color = palette.err;

    let w = &mut visuals.widgets;
    w.noninteractive.bg_fill = s.panel;
    w.noninteractive.weak_bg_fill = s.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, s.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, s.text);

    w.inactive.bg_fill = s.editor;
    w.inactive.weak_bg_fill = if dark { rgb(0x33_33_33) } else { rgb(0xFF_FF_FF) };
    w.inactive.bg_stroke = Stroke::new(1.0_f32, s.border);
    w.inactive.fg_stroke = Stroke::new(1.0_f32, s.text);

    w.hovered.bg_fill = s.hover;
    w.hovered.weak_bg_fill = s.hover;
    w.hovered.bg_stroke = Stroke::new(1.0_f32, s.focus);
    w.hovered.fg_stroke = Stroke::new(1.0_f32, s.text);

    w.active.bg_fill = s.active;
    w.active.weak_bg_fill = s.active;
    w.active.bg_stroke = Stroke::new(1.5_f32, s.focus);
    w.active.fg_stroke = Stroke::new(1.0_f32, if dark { rgb(0xFF_FF_FF) } else { s.text });

    w.open.bg_fill = s.hover;
    w.open.weak_bg_fill = s.hover;
    w.open.bg_stroke = Stroke::new(1.0_f32, s.border);
    w.open.fg_stroke = Stroke::new(1.0_f32, s.text);

    visuals.selection.bg_fill = s.selection;
    visuals.selection.stroke = Stroke::new(1.0_f32, s.focus);
    visuals
}

/// Applies a theme to the context and returns the matching accent palette.
pub fn apply(ctx: &Context, theme: Theme) -> Palette {
    let dark = theme.resolve(ctx.system_theme());
    let palette = Palette::for_dark(dark);
    ctx.style_mut(|style| {
        style.visuals = visuals(dark, &palette);
        for (_text_style, font) in style.text_styles.iter_mut() {
            font.size = font.size.max(15.0);
        }
        style.spacing.interact_size.y = style.spacing.interact_size.y.max(24.0);
    });
    palette
}
