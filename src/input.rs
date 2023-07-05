use super::*;

#[derive(Deserialize)]
pub struct Config {
    min_drag_distance: f64,
    max_click_time: f64,
    zoom_speed: f64,
}

pub struct Controller {
    ctx: crate::Context,
    config: Rc<Config>,
    cursor_pos: Option<vec2<f64>>,
    drag: Option<Drag>,
}

impl Controller {
    pub fn cursor_pos(&self) -> Option<vec2<f64>> {
        self.cursor_pos
    }
}

#[derive(Copy, Clone)]
struct Touch {
    id: u64,
    pos: vec2<f64>,
}

enum Drag {
    DetectPhase {
        start_position: vec2<f64>,
        timer: Timer,
        touch_id: Option<u64>,
    },
    Camera,
    Drag,
    Pinch {
        touches: [Touch; 2],
    },
}

pub enum State {
    Idle,
    Drag,
    TransformView,
}

impl Controller {
    pub fn new(ctx: &crate::Context) -> Self {
        Self {
            ctx: ctx.clone(),
            config: ctx.assets.config.input.clone(),
            cursor_pos: None,
            drag: None,
        }
    }
    pub fn state(&self) -> State {
        match self.drag {
            Some(Drag::Drag) => State::Drag,
            Some(Drag::Camera | Drag::Pinch { .. }) => State::TransformView,
            Some(Drag::DetectPhase { .. }) => State::Idle,
            None => State::Idle,
        }
    }
}

pub enum Event {
    DragStart(vec2<f64>),
    DragMove(vec2<f64>),
    DragEnd(vec2<f64>),
    Click(vec2<f64>),
    TransformView(TransformView),
    StopTransformView,
}

pub struct TransformView {
    pub from: vec2<f64>,
    pub to: vec2<f64>,
    pub fov_scale: f64,
    pub rotation: Angle<f64>,
}

impl TransformView {
    pub fn apply(&self, camera: &mut Camera2d, framebuffer_size: vec2<f32>) {
        let from = camera.screen_to_world(framebuffer_size, self.from.map(|x| x as f32));
        camera.fov *= self.fov_scale as f32;
        camera.rotation += self.rotation.map(|x| x as f32);
        let to = camera.screen_to_world(framebuffer_size, self.to.map(|x| x as f32));
        camera.center += from - to;
    }
}

pub trait Context {
    fn input(&mut self) -> &mut Controller;
    fn is_draggable(&self, screen_pos: vec2<f64>) -> bool;
    fn update(&mut self, _delta_time: f64) -> Vec<Event> {
        let state = self.input();
        if let &Some(Drag::DetectPhase {
            start_position,
            ref timer,
            ..
        }) = &state.drag
        {
            if timer.elapsed().as_secs_f64() > state.config.max_click_time {
                return self.start_drag(start_position);
            }
        }
        vec![]
    }

    fn handle_event(&mut self, event: geng::Event) -> Vec<Event> {
        let state = self.input();
        match event {
            geng::Event::MousePress { button: _ } => {
                if let Some(position) = self.input().cursor_pos {
                    self.input().drag = Some(Drag::DetectPhase {
                        start_position: position,
                        timer: Timer::new(),
                        touch_id: None,
                    });
                }
            }
            geng::Event::TouchStart(geng::Touch { id, position, .. }) => {
                if let Some(Drag::DetectPhase {
                    start_position,
                    touch_id: Some(other_id),
                    ..
                }) = state.drag
                {
                    state.drag = Some(Drag::Pinch {
                        touches: [
                            Touch {
                                id: other_id,
                                pos: start_position,
                            },
                            Touch { id, pos: position },
                        ],
                    });
                } else {
                    self.input().drag = Some(Drag::DetectPhase {
                        start_position: position,
                        timer: Timer::new(),
                        touch_id: Some(id),
                    });
                }
            }
            geng::Event::CursorMove { position, .. } => {
                return self.handle_move(position, None);
            }
            geng::Event::TouchMove(geng::Touch { id, position, .. }) => {
                return self.handle_move(position, Some(id));
            }
            geng::Event::MouseRelease { button: _ } => {
                if let Some(position) = state.cursor_pos {
                    if let Some(result) = self.handle_release(position) {
                        return result;
                    }
                }
            }
            geng::Event::TouchEnd(geng::Touch { position, .. }) => {
                if let Some(result) = self.handle_release(position) {
                    return result;
                }
            }
            geng::Event::Wheel { delta } => {
                let center = state.ctx.geng.window().size().map(|x| x as f64 / 2.0);
                return vec![Event::TransformView(TransformView {
                    from: state.cursor_pos.unwrap_or(center),
                    to: state.cursor_pos.unwrap_or(center),
                    fov_scale: (-delta * state.config.zoom_speed / 100.0).exp(),
                    rotation: Angle::ZERO,
                })];
            }
            _ => {}
        }
        vec![]
    }

    fn handle_release(&mut self, position: vec2<f64>) -> Option<Vec<Event>> {
        Some(match self.input().drag.take()? {
            Drag::DetectPhase { .. } => {
                vec![Event::Click(position)]
            }
            Drag::Drag => {
                vec![Event::DragEnd(position)]
            }
            Drag::Pinch { .. } | Drag::Camera => {
                vec![Event::StopTransformView]
            }
        })
    }

    fn start_drag(&mut self, position: vec2<f64>) -> Vec<Event> {
        if self.is_draggable(position) {
            self.input().drag = Some(Drag::Drag);
            vec![Event::DragStart(position)]
        } else {
            self.input().drag = Some(Drag::Camera);
            vec![]
        }
    }

    fn handle_move(&mut self, position: vec2<f64>, touch_id: Option<u64>) -> Vec<Event> {
        let input = self.input();
        let prev_pos = input.cursor_pos;
        input.cursor_pos = Some(position);
        let mut events = vec![];
        if let Some(Drag::DetectPhase { start_position, .. }) = input.drag {
            if (start_position - position).len() > input.config.min_drag_distance {
                // events.extend(self.start_drag(start_position));
                input.drag = Some(Drag::Camera);
            }
        }
        let state = self.input();
        match state.drag {
            Some(Drag::Drag) => {
                events.push(Event::DragMove(position));
            }
            Some(Drag::Camera) => {
                events.push(Event::TransformView(TransformView {
                    from: prev_pos.unwrap(),
                    to: position,
                    fov_scale: 1.0,
                    rotation: Angle::ZERO,
                }));
            }
            Some(Drag::Pinch {
                touches: old_touches,
            }) => {
                let mut new_touches = old_touches;
                for touch in &mut new_touches {
                    if Some(touch.id) == touch_id {
                        touch.pos = position;
                    }
                }
                state.drag = Some(Drag::Pinch {
                    touches: new_touches,
                });
                let center = |touches: [Touch; 2]| (touches[0].pos + touches[1].pos) / 2.0;
                let distance = |touches: [Touch; 2]| (touches[0].pos - touches[1].pos).len();
                let angle = |touches: [Touch; 2]| (touches[0].pos - touches[1].pos).arg();
                events.push(Event::TransformView(TransformView {
                    from: center(old_touches),
                    to: center(new_touches),
                    fov_scale: distance(old_touches) / distance(new_touches),
                    rotation: angle(new_touches) - angle(old_touches),
                }));
            }
            Some(Drag::DetectPhase { .. }) | None => {}
        }
        return events;
    }
}
