use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "Skylands: Untrapped".to_string(),
            width: 1280,
            height: 720,
        }
    }
}

pub trait RenderAppHandler {
    fn on_init(&mut self, window: Arc<Window>);
    fn on_resize(&mut self, width: u32, height: u32);
    fn on_update(&mut self);
    fn on_render(&mut self);
    fn on_key(&mut self, key_code: KeyCode, state: bool);
}

pub struct WindowApp<H: RenderAppHandler> {
    config: WindowConfig,
    handler: H,
    window: Option<Arc<Window>>,
}

impl<H: RenderAppHandler> WindowApp<H> {
    pub fn new(config: WindowConfig, handler: H) -> Self {
        Self {
            config,
            handler,
            window: None,
        }
    }

    pub fn run(mut self, event_loop: EventLoop<()>) -> Result<(), Box<dyn std::error::Error>> {
        event_loop.run_app(&mut self)?;
        Ok(())
    }
}

impl<H: RenderAppHandler> ApplicationHandler for WindowApp<H> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let attrs = WindowAttributes::default()
                .with_title(&self.config.title)
                .with_inner_size(winit::dpi::LogicalSize::new(
                    self.config.width,
                    self.config.height,
                ));
            if let Ok(window) = event_loop.create_window(attrs) {
                let arc_window = Arc::new(window);
                self.handler.on_init(arc_window.clone());
                self.window = Some(arc_window);
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                self.handler.on_resize(size.width, size.height);
            }
            WindowEvent::RedrawRequested => {
                self.handler.on_update();
                self.handler.on_render();
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state.is_pressed();
                if let PhysicalKey::Code(kc) = event.physical_key {
                    self.handler.on_key(kc, pressed);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}
