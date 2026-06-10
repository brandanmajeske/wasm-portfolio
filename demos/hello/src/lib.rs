use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

#[wasm_bindgen(start)]
pub fn run() -> Result<(), JsValue> {
    let document = web_sys::window()
        .ok_or("no window")?
        .document()
        .ok_or("no document")?;

    let canvas = document
        .get_element_by_id("hello-canvas")
        .ok_or("no #hello-canvas element")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;

    let ctx = canvas
        .get_context("2d")?
        .ok_or("no 2d context")?
        .dyn_into::<web_sys::CanvasRenderingContext2d>()?;

    let (w, h) = (canvas.width() as f64, canvas.height() as f64);

    // Background: vertical bars shaded by a little math, so the output is
    // unmistakably computed rather than a static image.
    let bars = 64;
    let bar_w = w / bars as f64;
    for i in 0..bars {
        let t = i as f64 / bars as f64;
        let lum = 18.0 + 14.0 * (t * std::f64::consts::TAU * 2.0).sin();
        ctx.set_fill_style_str(&format!("hsl(232, 35%, {lum}%)"));
        ctx.fill_rect(i as f64 * bar_w, 0.0, bar_w + 1.0, h);
    }

    ctx.set_fill_style_str("#7aa2f7");
    ctx.set_font("bold 32px system-ui, sans-serif");
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");
    ctx.fill_text("Hello from Rust 🦀 → WASM", w / 2.0, h / 2.0)?;

    Ok(())
}
