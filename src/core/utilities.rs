use chrono::Local;
use std::env;
use termimad::MadSkin;

pub fn username() -> String {
    let user = env::var("USER").unwrap_or_else(|_| "user".to_string());
    capitalize(&user)
}

pub fn datetime() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn capitalize(s: &str) -> String {
    s.get(0..1).unwrap_or("").to_uppercase() + s.get(1..).unwrap_or("")
}

pub enum RenderMode {
    Markdown,
    Plain,
}

pub fn render(response: &str, mode: RenderMode) -> String {
    match mode {
        RenderMode::Markdown => {
            let skin = MadSkin::default();
            skin.term_text(response).to_string()
        }
        RenderMode::Plain => response.to_string(),
    }
}
