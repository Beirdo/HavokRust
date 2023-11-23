use std::collections::HashMap;
use ansi_term::{Style, Color};
use fancy_regex::{Regex, RegexBuilder};
use tokio::sync::mpsc;
use lazy_static::lazy_static;
use std::sync::{Arc, RwLock};

use crate::logging::*;

#[derive(Debug, Clone, PartialEq, Copy)]
enum Styles {
    None,
    Bold,
    Faint,
    Italic,
    Underline,
    Blink,
    Negative,
}

#[derive(Debug, Clone)]
struct ColorMap {
    style: Styles,
    color: Color,
}


#[derive(Debug, Clone)]
struct RegexCapture {
    preamble: String,
    code: String,
    text: String,
    eol: String,
}

#[derive(Debug, Clone)]
struct AnsiParams {
    style: Vec<Styles>,
    bg: Color,
    fg: Color,
}

impl PartialEq for AnsiParams {
    fn eq(&self, other: &Self) -> bool {
        self.style == other.style && self.bg == other.bg && self.fg == other.fg
    }
}

impl Eq for AnsiParams {}

impl Default for AnsiParams {
    fn default() -> Self {
        AnsiParams {
            style: [Styles::None].to_vec(),
            bg: Color::Black,
            fg: Color::White,
        }
    }
}

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct AnsiColors {
    initialized: bool,
    default_color_code: String,
    fg_color_map: HashMap<String, ColorMap>,
    bg_color_map: HashMap<u32, Color>,
    style_map: HashMap<u32, Styles>,
    regex: Regex,
    logqueue: Option<mpsc::Sender<LogMessage>>,
}

lazy_static! {
    static ref ANSI_PARSER: Arc<RwLock<AnsiColors>> = Arc::new(RwLock::new(AnsiColors {
        initialized: false,
        default_color_code: "0007".to_string(),
        fg_color_map: HashMap::new(),
        bg_color_map: HashMap::new(),
        style_map: HashMap::new(),
        regex: Regex::new("").unwrap(),
        logqueue: None,
    }));
}

impl AnsiColors {
    pub fn get() -> Arc<RwLock<AnsiColors>> {
        let initialized = {
            ANSI_PARSER.read().unwrap().initialized
        };

        if !initialized {
            ANSI_PARSER.write().unwrap().initialize();
        }
        ANSI_PARSER.clone()
    }

    pub fn set_logqueue(logqueue: &mpsc::Sender<LogMessage>) {
        ANSI_PARSER.write().unwrap().logqueue = Some(logqueue.clone());        
    }

    fn initialize(&mut self) {
        let mut map = HashMap::new();
        map.insert("00".to_string(), ColorMap { style: Styles::None, color: Color::Black});
        map.insert("0X".to_string(), ColorMap { style: Styles::None, color: Color::Black});
        map.insert("01".to_string(), ColorMap { style: Styles::None, color: Color::Red});
        map.insert("0r".to_string(), ColorMap { style: Styles::None, color: Color::Red});
        map.insert("02".to_string(), ColorMap { style: Styles::None, color: Color::Green});
        map.insert("0g".to_string(), ColorMap { style: Styles::None, color: Color::Green});
        map.insert("03".to_string(), ColorMap { style: Styles::None, color: Color::Yellow});
        map.insert("0y".to_string(), ColorMap { style: Styles::None, color: Color::Yellow});
        map.insert("04".to_string(), ColorMap { style: Styles::None, color: Color::Blue});
        map.insert("0b".to_string(), ColorMap { style: Styles::None, color: Color::Blue});
        map.insert("05".to_string(), ColorMap { style: Styles::None, color: Color::Purple});
        map.insert("0p".to_string(), ColorMap { style: Styles::None, color: Color::Purple});
        map.insert("06".to_string(), ColorMap { style: Styles::None, color: Color::Cyan});
        map.insert("0c".to_string(), ColorMap { style: Styles::None, color: Color::Cyan});
        map.insert("07".to_string(), ColorMap { style: Styles::None, color: Color::White});
        map.insert("0w".to_string(), ColorMap { style: Styles::None, color: Color::White});
        map.insert("08".to_string(), ColorMap { style: Styles::Bold, color: Color::Black});
        map.insert("0x".to_string(), ColorMap { style: Styles::Bold, color: Color::Black});
        map.insert("09".to_string(), ColorMap { style: Styles::Bold, color: Color::Red});
        map.insert("0R".to_string(), ColorMap { style: Styles::Bold, color: Color::Red});
        map.insert("10".to_string(), ColorMap { style: Styles::Bold, color: Color::Green});
        map.insert("0G".to_string(), ColorMap { style: Styles::Bold, color: Color::Green});
        map.insert("11".to_string(), ColorMap { style: Styles::Bold, color: Color::Yellow});
        map.insert("0Y".to_string(), ColorMap { style: Styles::Bold, color: Color::Yellow});
        map.insert("12".to_string(), ColorMap { style: Styles::Bold, color: Color::Blue});
        map.insert("0B".to_string(), ColorMap { style: Styles::Bold, color: Color::Blue});
        map.insert("13".to_string(), ColorMap { style: Styles::Bold, color: Color::Purple});
        map.insert("0P".to_string(), ColorMap { style: Styles::Bold, color: Color::Purple});
        map.insert("14".to_string(), ColorMap { style: Styles::Bold, color: Color::Cyan});
        map.insert("0C".to_string(), ColorMap { style: Styles::Bold, color: Color::Cyan});
        map.insert("15".to_string(), ColorMap { style: Styles::Bold, color: Color::White});
        map.insert("0W".to_string(), ColorMap { style: Styles::Bold, color: Color::White});
        self.fg_color_map = map.clone();

        let mut bgmap = HashMap::new();
        bgmap.insert(0, Color::Black);
        bgmap.insert(1, Color::Red);
        bgmap.insert(2, Color::Green);
        bgmap.insert(3, Color::Yellow);
        bgmap.insert(4, Color::Blue);
        bgmap.insert(5, Color::Purple);
        bgmap.insert(6, Color::Cyan);
        bgmap.insert(7, Color::White);
        self.bg_color_map = bgmap.clone();

        let mut stymap = HashMap::new();
        stymap.insert(0, Styles::None);
        stymap.insert(1, Styles::Bold);
        stymap.insert(2, Styles::Faint);
        stymap.insert(3, Styles::Italic);
        stymap.insert(4, Styles::Underline);
        stymap.insert(5, Styles::Blink);
        stymap.insert(6, Styles::Negative);
        self.style_map = stymap.clone();

        self.default_color_code = "0007".to_string();
        self.regex = RegexBuilder::new(r"(?P<preamble>.*?)\$[Cc](?P<code>\d\d\d\S)(?P<text>.*?)(?=(?P<eol>[\r\n]+)|$|\$[Cc]\d\d\d\S)")
            .build()
            .unwrap();
        self.initialized = true;
    }

    pub fn convert_string(& self, message: String, ansi_mode: bool) -> Vec<u8> {
        let logqueue = self.logqueue.clone().unwrap();
        log_debug(&logqueue, &format!("Message: \"{}\", ANSI mode: {}", message, ansi_mode));
        let mut parts = vec![];
        let capt_iter = self.regex.captures_iter(&message);
        for capture in capt_iter {
            if !capture.is_err() {
                let cap = capture.unwrap();
                let part = RegexCapture {
                    preamble: cap.name("preamble").map_or("", |m| m.as_str()).to_string(),
                    code: cap.name("code").map_or("", |m| m.as_str()).to_string(),
                    text: cap.name("text").map_or("", |m| m.as_str()).to_string(),
                    eol: cap.name("eol").map_or("", |m| m.as_str()).to_string(),
                };
                parts.push(part);
            }
        }

        if parts.len() == 0 {
            let part = RegexCapture {
                preamble: "".to_string(),
                code: self.default_color_code.clone(),
                text: message.clone(),
                eol: "".to_string(),
            };
            parts.push(part);
        }

        log_debug(&logqueue, &format!("Parts: {:?}", parts));

        let mut new_parts = vec![];
        for part in parts {
            let mut old_part = part.clone();
            if old_part.preamble != "" {
                let new_part = RegexCapture {
                    preamble: "".to_string(),
                    code: self.default_color_code.clone(),
                    text: old_part.preamble.clone(),
                    eol: "".to_string(),
                };
                new_parts.push(new_part);
                old_part.preamble = "".to_string();
            }
            new_parts.push(old_part)
        }

        parts = new_parts.clone();

        log_debug(&logqueue, &format!("New Parts: {:?}", parts));

        let mut output: Vec<u8> = vec![];
        let mut have_old_color_params = false;
        let mut color_params: AnsiParams = Default::default();
        let mut old_color_params: AnsiParams = Default::default();

        for part in parts {
            if ansi_mode {
                let new_color_params = self.convert_code(part.code).clone();
                color_params = new_color_params.clone();
                log_debug(&logqueue, &format!("Color params: {:?}", color_params));
            }

            if !ansi_mode || (have_old_color_params && color_params == old_color_params) {
                output.append(&mut part.text.into_bytes());
            } else {
                let params = color_params.clone();
                let mut style = Style::new();
                
                for style_type in params.style {
                    match style_type {
                        Styles::None => {},
                        Styles::Bold => style = style.bold(),
                        Styles::Faint => style = style.dimmed(),
                        Styles::Italic => style = style.italic(),
                        Styles::Underline => style = style.underline(),
                        Styles::Blink => style = style.blink(),
                        Styles::Negative => style = style.reverse(),
                    };
                }

                style = style
                    .on(params.bg)
                    .fg(params.fg);

                log_debug(&logqueue, &format!("Style: {:?}", style));
                let ansi_string: String = format!("{}", style.paint(part.text));
                output.append(&mut ansi_string.into_bytes());
            }
            output.append(&mut part.eol.into_bytes());
            old_color_params = color_params.clone();
            have_old_color_params = true;
        }

        output
    }

    fn convert_code(& self, code: String) -> AnsiParams {
        let bg_index: &u32 = &code[1..2].parse().unwrap_or(0);
        let mut bg_color = self.bg_color_map.get(bg_index);
        if bg_color.is_none() {
            bg_color = Some(&Color::Black);
        }

        let style_index: &u32 = &code[0..1].parse().unwrap_or(0);
        let mut style = self.style_map.get(style_index);
        if style.is_none() {
            style = Some(&Styles::None);
        }

        
        let mut fg_code = &code[2..];
        let mut fg_item = self.fg_color_map.get(fg_code);
        if fg_item.is_none() {
            // Go with the default
            fg_code = &self.default_color_code[2..];
            fg_item = self.fg_color_map.get(fg_code);
        }

        let fg_color = fg_item.unwrap();
        let fg_style = fg_color.style.clone();
        let new_style = *style.clone().unwrap();

        let mut styles = vec![];
        if fg_style == Styles::None && new_style == Styles::Bold {
            styles.push(fg_style);
        } else if new_style == Styles::None {
            styles.push(fg_style);
        } else {
            styles.push(new_style);
            styles.push(fg_style);
        }

        AnsiParams {
            style: styles.clone(),
            bg: bg_color.unwrap().clone(),
            fg: fg_color.color.clone(),
        }
    }

}
