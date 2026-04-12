use rustic_render::gpu::GpuState;
use winit::keyboard::KeyCode;

#[derive(Debug, Clone)]
pub struct GameplayChangersState {
    pub selected: usize,
    pub practice_mode: bool,
    pub botplay: bool,
}

impl GameplayChangersState {
    pub fn new(practice_mode: bool, botplay: bool) -> Self {
        Self {
            selected: 0,
            practice_mode,
            botplay,
        }
    }

    pub fn draw(&self, gpu: &mut GpuState) {
        gpu.push_colored_quad(180.0, 160.0, 920.0, 360.0, [0.0, 0.0, 0.0, 0.86]);
        gpu.draw_batch(None);

        gpu.draw_text("GAMEPLAY CHANGERS", 320.0, 200.0, 34.0, [1.0, 1.0, 1.0, 1.0]);
        gpu.draw_text(
            "Press CTRL or ESCAPE to close",
            320.0,
            240.0,
            20.0,
            [0.75, 0.75, 0.75, 1.0],
        );

        for (idx, line) in self.lines().iter().enumerate() {
            let y = 320.0 + idx as f32 * 70.0;
            let color = if idx == self.selected {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                [0.7, 0.7, 0.7, 1.0]
            };
            let prefix = if idx == self.selected { "> " } else { "  " };
            gpu.draw_text(&format!("{prefix}{line}"), 320.0, y, 28.0, color);
        }
    }

    pub fn handle_key(&mut self, key: KeyCode) -> bool {
        match key {
            KeyCode::ArrowUp | KeyCode::KeyW => {
                self.selected = (self.selected + self.lines().len() - 1) % self.lines().len();
                true
            }
            KeyCode::ArrowDown | KeyCode::KeyS => {
                self.selected = (self.selected + 1) % self.lines().len();
                true
            }
            KeyCode::ArrowLeft
            | KeyCode::ArrowRight
            | KeyCode::KeyA
            | KeyCode::KeyD
            | KeyCode::Enter
            | KeyCode::Space => {
                match self.selected {
                    0 => self.practice_mode = !self.practice_mode,
                    1 => self.botplay = !self.botplay,
                    _ => {}
                }
                true
            }
            _ => false,
        }
    }

    fn lines(&self) -> [String; 2] {
        [
            format!("Practice Mode        [ {} ]", on_off(self.practice_mode)),
            format!("Botplay              [ {} ]", on_off(self.botplay)),
        ]
    }
}

fn on_off(enabled: bool) -> &'static str {
    if enabled { "ON" } else { "OFF" }
}

