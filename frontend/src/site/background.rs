//! Animated synthwave grid + particles, drawn on a full-screen canvas (Background.tsx).
use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use yew::prelude::*;

struct Particle {
    x: f64,
    y: f64,
    s: f64,
    v: f64,
}

#[component]
pub fn Background() -> Html {
    let canvas = use_node_ref();
    {
        let canvas = canvas.clone();
        use_effect_with((), move |_| {
            let mut cleanup: Option<Box<dyn FnOnce()>> = None;
            let el = canvas.cast::<web_sys::HtmlCanvasElement>();
            let ctx =
                el.as_ref().and_then(|c| c.get_context("2d").ok().flatten()).and_then(|c| c.dyn_into::<web_sys::CanvasRenderingContext2d>().ok());
            if let (Some(el), Some(ctx)) = (el, ctx) {
                let w = web_sys::window().unwrap();
                let reduce = w.match_media("(prefers-reduced-motion: reduce)").ok().flatten().is_some_and(|m| m.matches());
                let dpr = Some(w.device_pixel_ratio()).filter(|r| *r > 0.0).unwrap_or(1.0).min(2.0);
                let rnd = js_sys::Math::random;
                let particles: Rc<RefCell<Vec<Particle>>> = Rc::new(RefCell::new(
                    (0..70).map(|_| Particle { x: rnd(), y: rnd(), s: rnd() * 1.6 + 0.3, v: rnd() * 0.0004 + 0.0001 }).collect(),
                ));
                let size = Rc::new(RefCell::new((0.0f64, 0.0f64)));
                let resize = {
                    let (el, ctx, size) = (el.clone(), ctx.clone(), size.clone());
                    move || {
                        let w = web_sys::window().unwrap();
                        let (iw, ih) = (
                            w.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(0.0),
                            w.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(0.0),
                        );
                        *size.borrow_mut() = (iw, ih);
                        el.set_width((iw * dpr) as u32);
                        el.set_height((ih * dpr) as u32);
                        let _ = ctx.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0);
                    }
                };
                resize();
                let on_resize = gloo::events::EventListener::new(&w, "resize", move |_| resize());
                let raf: Rc<RefCell<i32>> = Rc::new(RefCell::new(0));
                let frame: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>> = Rc::new(RefCell::new(None));
                {
                    let (frame2, raf2) = (frame.clone(), raf.clone());
                    *frame.borrow_mut() = Some(Closure::new(move |t: f64| {
                        let (w, h) = *size.borrow();
                        ctx.clear_rect(0.0, 0.0, w, h);
                        let horizon = h * 0.55;
                        let offset = if reduce { 0.0 } else { (t / 2200.0) % 1.0 };
                        let grad = ctx.create_linear_gradient(0.0, horizon, 0.0, h);
                        let _ = grad.add_color_stop(0.0, "rgba(0,240,255,0)");
                        let _ = grad.add_color_stop(0.4, "rgba(0,240,255,0.18)");
                        let _ = grad.add_color_stop(1.0, "rgba(255,43,214,0.28)");
                        ctx.set_stroke_style_canvas_gradient(&grad);
                        ctx.set_line_width(1.0);
                        ctx.begin_path();
                        for i in 0..22 {
                            let p = (i as f64 + offset) / 22.0;
                            let y = horizon + (h - horizon) * p * p;
                            ctx.move_to(0.0, y);
                            ctx.line_to(w, y);
                        }
                        for i in -16..=16 {
                            let i = i as f64;
                            ctx.move_to(w / 2.0 + i * 8.0, horizon);
                            ctx.line_to(w / 2.0 + (i * w) / 5.0, h);
                        }
                        ctx.stroke();
                        for p in particles.borrow_mut().iter_mut() {
                            if !reduce {
                                p.y -= p.v;
                            }
                            if p.y < 0.0 {
                                p.y = 1.0;
                            }
                            ctx.set_fill_style_str(&format!("rgba(160,240,255,{})", 0.25 + p.s / 4.0));
                            ctx.fill_rect(p.x * w, p.y * h, p.s, p.s);
                        }
                        if !reduce {
                            if let Some(f) = frame2.borrow().as_ref() {
                                *raf2.borrow_mut() = web_sys::window().unwrap().request_animation_frame(f.as_ref().unchecked_ref()).unwrap_or(0);
                            }
                        }
                    }));
                }
                *raf.borrow_mut() = w.request_animation_frame(frame.borrow().as_ref().unwrap().as_ref().unchecked_ref()).unwrap_or(0);
                cleanup = Some(Box::new(move || {
                    let _ = web_sys::window().unwrap().cancel_animation_frame(*raf.borrow());
                    drop(on_resize);
                    // Break the closure's self-reference so it is freed.
                    frame.borrow_mut().take();
                }));
            }
            move || {
                if let Some(c) = cleanup {
                    c();
                }
            }
        });
    }
    html! {
        <div class="bg" aria-hidden="true">
            <div class="orb orb-a" />
            <div class="orb orb-b" />
            <canvas ref={canvas} />
            <div class="scanlines" />
        </div>
    }
}
