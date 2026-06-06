use gpui::{Font, FontFeatures, FontStyle, FontWeight, Hsla, Pixels, SharedString, TextRun, px};

use crate::model::FountainElement;

pub const COURIER_PRIME_REGULAR: &[u8] =
    include_bytes!("../../assets/fonts/CourierPrime-Regular.ttf");
pub const COURIER_PRIME_BOLD: &[u8] =
    include_bytes!("../../assets/fonts/CourierPrime-Bold.ttf");
pub const COURIER_PRIME_ITALIC: &[u8] =
    include_bytes!("../../assets/fonts/CourierPrime-Italic.ttf");
pub const COURIER_PRIME_BOLD_ITALIC: &[u8] =
    include_bytes!("../../assets/fonts/CourierPrime-BoldItalic.ttf");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

pub struct ElementStyle {
    pub font_family: SharedString,
    pub font_weight: FontWeight,
    pub font_style: FontStyle,
    pub font_size: Pixels,
    pub line_height: Pixels,
    pub left_pad: Pixels,
    pub right_pad: Pixels,
    pub text_align: TextAlign,
    pub color: Hsla,
    pub uppercase: bool,
}

const COURIER_PRIME: &str = "Courier Prime";
const SYSTEM_UI: &str = ".SystemUIFont";

// Lightness values against the #1c1c1c background
const WHITE: Hsla = Hsla { h: 0.0, s: 0.0, l: 1.00, a: 1.0 }; // scene headings
const WHITE_NORMAL: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.90, a: 1.0 }; // body text
const WHITE_OFF: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.82, a: 1.0 }; // section h2
const WHITE_MUTED: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.70, a: 1.0 }; // section h3+
const GRAY_NOTE: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.53, a: 1.0 }; // notes (#888)

// Standard Fountain margins on a 700px page (700 / 8.5in ≈ 82.35 px/in)
const LEFT_ACTION: f32 = 124.0;  // 1.5"
const RIGHT_ACTION: f32 = 82.0;  // 1.0"
const LEFT_CHARACTER: f32 = 305.0; // 3.7"
const LEFT_DIALOGUE: f32 = 206.0; // 2.5"
const RIGHT_DIALOGUE: f32 = 165.0; // 2.0"
const LEFT_PAREN: f32 = 247.0;   // 3.0"
const RIGHT_PAREN: f32 = 206.0;  // 2.5"

fn action_style() -> ElementStyle {
    ElementStyle {
        font_family: COURIER_PRIME.into(),
        font_weight: FontWeight::NORMAL,
        font_style: FontStyle::Normal,
        font_size: px(12.0),
        line_height: px(22.0),
        left_pad: px(LEFT_ACTION),
        right_pad: px(RIGHT_ACTION),
        text_align: TextAlign::Left,
        color: WHITE_NORMAL,
        uppercase: false,
    }
}

pub fn element_style(element: &FountainElement) -> ElementStyle {
    match element {
        FountainElement::SceneHeading => ElementStyle {
            font_weight: FontWeight::BOLD,
            font_size: px(13.0),
            line_height: px(24.0),
            color: WHITE,
            uppercase: true,
            ..action_style()
        },

        FountainElement::Action => action_style(),

        FountainElement::Character => ElementStyle {
            left_pad: px(LEFT_CHARACTER),
            right_pad: px(RIGHT_ACTION),
            uppercase: true,
            ..action_style()
        },

        FountainElement::Dialogue => ElementStyle {
            left_pad: px(LEFT_DIALOGUE),
            right_pad: px(RIGHT_DIALOGUE),
            ..action_style()
        },

        FountainElement::Parenthetical => ElementStyle {
            left_pad: px(LEFT_PAREN),
            right_pad: px(RIGHT_PAREN),
            font_style: FontStyle::Italic,
            ..action_style()
        },

        FountainElement::Transition => ElementStyle {
            left_pad: px(LEFT_ACTION),
            right_pad: px(RIGHT_ACTION),
            text_align: TextAlign::Right,
            uppercase: true,
            ..action_style()
        },

        FountainElement::Lyric => ElementStyle {
            font_style: FontStyle::Italic,
            ..action_style()
        },

        FountainElement::Note => ElementStyle {
            font_family: SYSTEM_UI.into(),
            font_size: px(11.0),
            line_height: px(20.0),
            font_style: FontStyle::Italic,
            color: GRAY_NOTE,
            ..action_style()
        },

        FountainElement::Section(depth) => ElementStyle {
            font_family: SYSTEM_UI.into(),
            font_weight: FontWeight::BOLD,
            font_size: match depth {
                1 => px(16.0),
                2 => px(14.0),
                _ => px(13.0),
            },
            line_height: match depth {
                1 => px(28.0),
                2 => px(26.0),
                _ => px(24.0),
            },
            color: match depth {
                1 => WHITE,
                2 => WHITE_OFF,
                _ => WHITE_MUTED,
            },
            ..action_style()
        },

        // PageBreak renders as a rule, not text; TitlePage has its own view.
        FountainElement::PageBreak | FountainElement::TitlePage => action_style(),
    }
}

pub fn element_to_text_run(text: &str, element: &FountainElement) -> Vec<TextRun> {
    let style = element_style(element);
    vec![TextRun {
        len: text.len(),
        font: Font {
            family: style.font_family,
            weight: style.font_weight,
            style: style.font_style,
            features: FontFeatures::default(),
            fallbacks: None,
        },
        color: style.color,
        background_color: None,
        underline: None,
        strikethrough: None,
    }]
}
