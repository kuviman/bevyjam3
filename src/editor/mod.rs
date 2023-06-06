use super::*;

#[derive(Deserialize)]
pub struct Controls {
    pub toggle: geng::Key,
    camera_drag: geng::MouseButton,
    create: geng::MouseButton,
    delete: geng::MouseButton,
    choose: geng::Key,
    pick: geng::Key,
    grid: geng::Key,
    rotate: geng::Key,
}

#[derive(Deserialize)]
struct BrushWheelConfig {
    radius: f32,
    inner_radius: f32,
    color: Rgba<f32>,
}

#[derive(Deserialize)]
pub struct Config {
    default_fov: f32,
    min_fov: f32,
    max_fov: f32,
    zoom_speed: f32,
    index_size: f32,
    index_color: Rgba<f32>,
    grid_color: Rgba<f32>,
    brush_preview_opacity: f32,
    brush_wheel: BrushWheelConfig,
    pub controls: Controls,
}

enum BrushType {
    Entity(String),
    Tile(Tile),
    Powerup(Effect),
    Goal,
}

impl BrushType {
    fn tile_name(&self) -> String {
        match self {
            Self::Entity(name) => name.clone(),
            Self::Tile(tile) => format!("{tile:?}").to_lowercase(),
            Self::Powerup(effect) => format!("{effect:?}Power"),
            Self::Goal => "Goal".to_owned(),
        }
    }
}

struct Brush {
    angle: IntAngle,
    brush_type: BrushType,
}

impl Brush {
    fn rotation(&self) -> f32 {
        // TODO normalize angles in the codebase
        let angle = self.angle;
        match self.brush_type {
            BrushType::Entity(_) => angle,
            BrushType::Tile(_) => angle,
            BrushType::Powerup(_) => angle.rotate_counter_clockwise(),
            BrushType::Goal => angle,
        }
        .to_radians()
    }
    fn pick(state: &GameState, cell: vec2<i32>) -> Option<Self> {
        if let Some(tile) = state.tiles.get(&cell) {
            return Some(Self {
                angle: IntAngle::RIGHT,
                brush_type: BrushType::Tile(*tile),
            });
        }
        if let Some(entity) = state.entities.iter().find(|entity| entity.pos.cell == cell) {
            return Some(Self {
                angle: entity.pos.angle,
                brush_type: BrushType::Entity(entity.identifier.clone()),
            });
        }
        if let Some(powerup) = state
            .powerups
            .iter()
            .find(|powerup| powerup.pos.cell == cell)
        {
            return Some(Self {
                angle: powerup.pos.angle,
                brush_type: BrushType::Powerup(powerup.effect.clone()),
            });
        }
        if let Some(goal) = state.goals.iter().find(|goal| goal.pos.cell == cell) {
            return Some(Self {
                angle: goal.pos.angle,
                brush_type: BrushType::Goal,
            });
        }
        None
    }
}

struct BrushWheelItem {
    brush: Brush,
    pos: vec2<f32>,
    hovered: bool,
}

pub struct State {
    framebuffer_size: vec2<f32>,
    geng: Geng,
    assets: Rc<Assets>,
    game_state: GameState,
    camera: Camera2d,
    transition: Option<geng::state::Transition>,
    sound: Rc<sound::State>,
    renderer: Rc<Renderer>,
    level_mesh: renderer::LevelMesh,
    finish_callback: play::FinishCallback,
    camera_drag: Option<vec2<f64>>,
    brush: Brush,
    brush_wheel_pos: Option<vec2<f32>>,
    path: std::path::PathBuf,
    history: Vec<GameState>,
    show_grid: bool,
}

impl State {
    pub fn new(
        geng: &Geng,
        assets: &Rc<Assets>,
        renderer: &Rc<Renderer>,
        sound: &Rc<sound::State>,
        game_state: Option<GameState>,
        path: impl AsRef<std::path::Path>,
        finish_callback: play::FinishCallback,
    ) -> Self {
        let path = path.as_ref();
        // TODO: block_on doesnt work on the web
        let game_state: GameState = game_state
            .unwrap_or_else(|| futures::executor::block_on(file::load_detect(path)).unwrap());
        Self {
            path: path.to_owned(),
            geng: geng.clone(),
            assets: assets.clone(),
            framebuffer_size: vec2::splat(1.0),
            camera: Camera2d {
                center: game_state.center(),
                rotation: 0.0,
                fov: assets.config.editor.default_fov,
            },
            transition: None,
            sound: sound.clone(),
            renderer: renderer.clone(),
            level_mesh: renderer.level_mesh(&game_state),
            finish_callback,
            camera_drag: None,
            brush: Brush {
                angle: IntAngle::RIGHT,
                brush_type: BrushType::Entity("Player".to_owned()),
            },
            brush_wheel_pos: None,
            history: vec![game_state.clone()],
            game_state,
            show_grid: false,
        }
    }

    fn screen_to_tile(&self, screen_pos: vec2<f64>) -> vec2<i32> {
        let world_pos = self
            .camera
            .screen_to_world(self.framebuffer_size, screen_pos.map(|x| x as f32));
        world_pos.map(|x| x.floor() as i32)
    }

    fn create(&mut self, screen_pos: vec2<f64>) {
        self.delete(screen_pos);
        let cell = self.screen_to_tile(screen_pos);
        match &self.brush.brush_type {
            BrushType::Entity(name) => self.game_state.add_entity(
                name,
                &self.assets.logic_config.entities[name],
                Position {
                    cell,
                    angle: self.brush.angle,
                },
            ),
            BrushType::Tile(tile) => {
                self.game_state.tiles.insert(cell, *tile);
                self.level_mesh = self.renderer.level_mesh(&self.game_state);
            }
            BrushType::Powerup(effect) => {
                self.game_state.powerups.insert(Powerup {
                    id: self.game_state.id_gen.gen(),
                    pos: Position {
                        cell,
                        angle: self.brush.angle,
                    },
                    effect: effect.clone(),
                });
            }
            BrushType::Goal => self.game_state.goals.insert(Goal {
                id: self.game_state.id_gen.gen(),
                pos: Position {
                    cell,
                    angle: self.brush.angle,
                },
            }),
        }
    }

    fn delete(&mut self, screen_pos: vec2<f64>) {
        let tile = self.screen_to_tile(screen_pos);
        if self.game_state.tiles.remove(&tile).is_some() {
            self.level_mesh = self.renderer.level_mesh(&self.game_state);
        }
        self.game_state
            .entities
            .retain(|entity| entity.pos.cell != tile);
        self.game_state
            .powerups
            .retain(|entity| entity.pos.cell != tile);
        self.game_state
            .goals
            .retain(|entity| entity.pos.cell != tile);
    }

    fn brush_wheel(&self) -> Option<impl Iterator<Item = BrushWheelItem> + '_> {
        let center = self.brush_wheel_pos?;
        let entities = self
            .assets
            .logic_config
            .entities
            .keys()
            .map(|name| BrushType::Entity(name.clone()))
            .map(|brush_type| Brush {
                angle: IntAngle::RIGHT,
                brush_type,
            });
        let tiles = Tile::iter_variants()
            .map(BrushType::Tile)
            .map(|brush_type| Brush {
                angle: IntAngle::RIGHT,
                brush_type,
            });
        let powerups = Effect::iter_variants()
            .map(BrushType::Powerup)
            .map(|brush_type| Brush {
                angle: IntAngle::DOWN,
                brush_type,
            });
        let goal = Brush {
            angle: IntAngle::RIGHT,
            brush_type: BrushType::Goal,
        };

        let mut items: Vec<BrushWheelItem> = itertools::chain![entities, tiles, powerups, [goal]]
            .map(|brush| BrushWheelItem {
                brush,
                pos: vec2::ZERO,
                hovered: false,
            })
            .collect();
        let len = items.len();
        for (index, item) in items.iter_mut().enumerate() {
            item.pos = center
                + vec2(self.assets.config.editor.brush_wheel.radius, 0.0)
                    .rotate(2.0 * f32::PI * index as f32 / len as f32);
        }
        let cursor_delta = self.camera.screen_to_world(
            self.framebuffer_size,
            self.geng.window().cursor_position().map(|x| x as f32),
        ) - center;
        if cursor_delta.len() > self.assets.config.editor.brush_wheel.inner_radius {
            if let Some(item) = items
                .iter_mut()
                .filter(|item| vec2::dot(item.pos - center, cursor_delta) > 0.0)
                .min_by_key(|item| r32(vec2::skew(item.pos - center, cursor_delta).abs()))
            {
                item.hovered = true;
            }
        }
        Some(items.into_iter())
    }

    fn save(&mut self) {
        // TODO saved flag & warning
        ron::ser::to_writer_pretty(
            std::io::BufWriter::new(std::fs::File::create(&self.path).unwrap()),
            &self.game_state,
            default(),
        )
        .unwrap();
    }

    fn undo(&mut self) {
        if self.history.len() > 1 {
            if self.game_state != self.history.pop().unwrap() {
                log::error!("DID YOU JUST CTRL-Z WHILE PAINTING?");
            }
            self.game_state = self.history.last().unwrap().clone();
            self.level_mesh = self.renderer.level_mesh(&self.game_state);
        }
    }

    fn push_history_if_needed(&mut self) {
        if self.game_state != *self.history.last().unwrap() {
            log::debug!("Pushed history");
            self.history.push(self.game_state.clone());
        }
    }

    fn assign_index(&mut self, index: i32) {
        let cell = self.screen_to_tile(self.geng.window().cursor_position());
        if let Some(entity) = self
            .game_state
            .entities
            .iter_mut()
            .find(|entity| entity.pos.cell == cell)
        {
            entity.index = Some(index);
        }
        self.push_history_if_needed();
    }
}

impl Drop for State {
    fn drop(&mut self) {
        self.save();
    }
}

impl geng::State for State {
    fn update(&mut self, delta_time: f64) {
        let _delta_time = delta_time as f32;
    }
    fn transition(&mut self) -> Option<geng::state::Transition> {
        self.transition.take()
    }
    fn handle_event(&mut self, event: geng::Event) {
        let controls = &self.assets.config.editor.controls;
        match event {
            geng::Event::KeyDown { key } if key == controls.grid => {
                self.show_grid = !self.show_grid;
            }
            geng::Event::KeyDown { key } if key == controls.toggle => {
                self.transition =
                    Some(geng::state::Transition::Switch(Box::new(play::State::new(
                        &self.geng,
                        &self.assets,
                        &self.renderer,
                        &self.sound,
                        self.game_state.clone(),
                        self.finish_callback.clone(),
                    ))));
            }
            geng::Event::KeyDown { key } if key == controls.choose => {
                self.brush_wheel_pos = Some(self.camera.screen_to_world(
                    self.framebuffer_size,
                    self.geng.window().cursor_position().map(|x| x as f32),
                ));
            }
            geng::Event::KeyUp { key } if key == controls.choose => {
                let hovered_item = self
                    .brush_wheel()
                    .into_iter()
                    .flatten()
                    .find(|item| item.hovered);
                if let Some(item) = hovered_item {
                    self.brush = item.brush;
                }
                self.brush_wheel_pos = None;
            }
            geng::Event::KeyDown { key } if key == controls.pick => {
                if let Some(brush) = Brush::pick(
                    &self.game_state,
                    self.screen_to_tile(self.geng.window().cursor_position()),
                ) {
                    self.brush = brush;
                }
            }
            geng::Event::MouseDown { position, button } if button == controls.create => {
                self.create(position);
            }
            geng::Event::MouseDown { position, button } if button == controls.delete => {
                self.delete(position);
            }
            geng::Event::MouseUp { button, .. }
                if [controls.create, controls.delete].contains(&button) =>
            {
                self.push_history_if_needed();
            }
            geng::Event::MouseDown { position, button } if button == controls.camera_drag => {
                self.camera_drag = Some(position);
            }
            geng::Event::MouseUp { button, .. } if button == controls.camera_drag => {
                self.camera_drag = None;
            }
            geng::Event::MouseMove { position, .. } => {
                if self.geng.window().is_button_pressed(controls.create) {
                    self.create(position);
                } else if self.geng.window().is_button_pressed(controls.delete) {
                    self.delete(position);
                } else if let Some(drag) = &mut self.camera_drag {
                    let world_pos = |pos: vec2<f64>| -> vec2<f32> {
                        self.camera
                            .screen_to_world(self.framebuffer_size, pos.map(|x| x as f32))
                    };
                    let before = world_pos(*drag);
                    let now = world_pos(position);
                    self.camera.center += before - now;
                    *drag = position;
                }
            }
            geng::Event::Wheel { delta } => {
                let before = self.camera.screen_to_world(
                    self.framebuffer_size,
                    self.geng.window().cursor_position().map(|x| x as f32),
                );
                self.camera.fov =
                    (self.camera.fov - delta as f32 * self.assets.config.editor.zoom_speed).clamp(
                        self.assets.config.editor.min_fov,
                        self.assets.config.editor.max_fov,
                    );
                let now = self.camera.screen_to_world(
                    self.framebuffer_size,
                    self.geng.window().cursor_position().map(|x| x as f32),
                );
                self.camera.center += before - now;
            }
            geng::Event::KeyDown { key } if key == controls.rotate => {
                let mut delta = 1;
                if self.geng.window().is_key_pressed(geng::Key::LShift) {
                    delta = -delta;
                }
                self.brush.angle = self.brush.angle.with_input(Input::from_sign(delta));
            }
            geng::Event::KeyDown { key: geng::Key::S }
                if self.geng.window().is_key_pressed(geng::Key::LCtrl) =>
            {
                self.save();
            }
            geng::Event::KeyDown { key: geng::Key::Z }
                if self.geng.window().is_key_pressed(geng::Key::LCtrl) =>
            {
                self.undo();
            }

            // TODO: macro?
            geng::Event::KeyDown {
                key: geng::Key::Num1,
            } => {
                self.assign_index(1);
            }
            geng::Event::KeyDown {
                key: geng::Key::Num2,
            } => {
                self.assign_index(2);
            }
            geng::Event::KeyDown {
                key: geng::Key::Num3,
            } => {
                self.assign_index(3);
            }
            geng::Event::KeyDown {
                key: geng::Key::Num4,
            } => {
                self.assign_index(4);
            }
            geng::Event::KeyDown {
                key: geng::Key::Num5,
            } => {
                self.assign_index(5);
            }
            geng::Event::KeyDown {
                key: geng::Key::Num6,
            } => {
                self.assign_index(6);
            }
            geng::Event::KeyDown {
                key: geng::Key::Num7,
            } => {
                self.assign_index(7);
            }
            geng::Event::KeyDown {
                key: geng::Key::Num8,
            } => {
                self.assign_index(8);
            }
            geng::Event::KeyDown {
                key: geng::Key::Num9,
            } => {
                self.assign_index(9);
            }

            _ => {}
        }
    }
    fn draw(&mut self, framebuffer: &mut ugli::Framebuffer) {
        self.framebuffer_size = framebuffer.size().map(|x| x as f32);
        self.renderer.draw(
            framebuffer,
            &self.camera,
            history::Frame {
                current_state: &self.game_state,
                animation: None,
            },
            &self.level_mesh,
        );

        for entity in &self.game_state.entities {
            if let Some(index) = entity.index {
                self.geng.default_font().draw(
                    framebuffer,
                    &self.camera,
                    &index.to_string(),
                    vec2::splat(geng::TextAlign::CENTER),
                    mat3::translate(entity.pos.cell.map(|x| x as f32 + 0.5))
                        * mat3::scale_uniform(self.assets.config.editor.index_size),
                    self.assets.config.editor.index_color,
                );
            }
        }

        if self.show_grid {
            self.renderer.draw_grid(
                framebuffer,
                &self.camera,
                self.assets.config.editor.grid_color,
            );
        }

        self.renderer.draw_tile(
            framebuffer,
            &self.camera,
            &self.brush.brush_type.tile_name(),
            Rgba::new(
                1.0,
                1.0,
                1.0,
                self.assets.config.editor.brush_preview_opacity,
            ),
            mat3::translate(
                self.screen_to_tile(self.geng.window().cursor_position())
                    .map(|x| x as f32),
            ) * mat3::rotate_around(vec2::splat(0.5), self.brush.rotation()),
        );
        self.renderer.draw_tile(
            framebuffer,
            &self.camera,
            "EditorSelect",
            Rgba::WHITE,
            mat3::translate(
                self.screen_to_tile(self.geng.window().cursor_position())
                    .map(|x| x as f32),
            ),
        );

        if let Some(wheel) = self.brush_wheel() {
            let center = self.brush_wheel_pos.unwrap();
            let config = &self.assets.config.editor.brush_wheel;
            self.geng.draw2d().draw2d(
                framebuffer,
                &self.camera,
                &draw2d::Ellipse::circle_with_cut(
                    center,
                    config.inner_radius,
                    2.0 * config.radius - config.inner_radius,
                    config.color,
                ),
            );
            for item in wheel {
                self.renderer.draw_tile(
                    framebuffer,
                    &self.camera,
                    &item.brush.brush_type.tile_name(),
                    Rgba::WHITE,
                    mat3::translate(item.pos)
                        * mat3::scale_uniform(if item.hovered { 2.0 } else { 1.0 })
                        * mat3::rotate(item.brush.rotation())
                        * mat3::translate(vec2::splat(-0.5)),
                );
            }
        }
    }
}
