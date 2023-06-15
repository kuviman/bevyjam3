use super::*;

#[derive(Deserialize)]
pub struct Config {
    fov: f32,
    ui_fov: f32,
}

pub struct State {
    ctx: Context,
    framebuffer_size: vec2<f32>,
    camera: Camera2d,
    ui_camera: Camera2d,
    transition: Option<Transition>,
    level_mesh: renderer::LevelMesh,
    history_player: history::Player,
    vfx: renderer::Vfx,
    next_zzz: f32,
    zzz: bool,
    touch_input: Option<Input>,
    buttons: Box<[Button<ButtonType>]>,
}

pub enum Transition {
    NextLevel,
    PrevLevel,
    Editor,
    Exit,
}

enum ButtonType {
    Undo,
    Redo,
    Reset,
    Exit,
    SwitchPlayer,
}

impl State {
    pub fn new(ctx: &Context, level: &Level) -> Self {
        let game_state = GameState::init(&ctx.assets.logic_config, level);
        let config = &ctx.assets.config.play;
        Self {
            ctx: ctx.clone(),
            framebuffer_size: vec2::splat(1.0),
            camera: Camera2d {
                center: game_state.center(),
                rotation: Angle::ZERO,
                fov: config.fov,
            },
            ui_camera: Camera2d {
                center: vec2::ZERO,
                rotation: Angle::ZERO,
                fov: config.ui_fov,
            },
            transition: None,
            level_mesh: ctx.renderer.level_mesh(level),
            history_player: history::Player::new(
                game_state,
                &ctx.assets.logic_config,
                ctx.assets.config.animation_time,
            ),
            vfx: renderer::Vfx::new(ctx),
            next_zzz: ctx.assets.config.zzz_time,
            zzz: false,
            touch_input: None,
            buttons: Box::new([
                Button::square(Anchor::TOP_RIGHT, vec2(-1, -1), ButtonType::Exit),
                Button::square(Anchor::BOTTOM_LEFT, vec2(0, 0), ButtonType::Reset),
                Button::square(Anchor::BOTTOM_LEFT, vec2(2, 0), ButtonType::Undo),
                Button::square(Anchor::BOTTOM_LEFT, vec2(3, 0), ButtonType::Redo),
                Button::square(Anchor::TOP_LEFT, vec2(1, -1), ButtonType::SwitchPlayer),
            ]),
        }
    }
    pub fn finish(&mut self, finish: Transition) {
        self.transition = Some(finish);
    }

    pub async fn run(mut self, actx: &mut async_states::Context) -> Transition {
        loop {
            let flow = match actx.wait().await {
                async_states::Event::Event(event) => self.handle_event(event),
                async_states::Event::Update(delta_time) => self.update(delta_time),
                async_states::Event::Draw => {
                    self.draw(&mut actx.framebuffer());
                    ControlFlow::Continue(())
                }
            };
            if let ControlFlow::Break(()) = flow {
                return Transition::Exit;
            }
            if let Some(transition) = self.transition.take() {
                return transition;
            }
        }
    }

    fn update(&mut self, delta_time: f64) -> ControlFlow<()> {
        let delta_time = delta_time as f32;

        let is_pressed = |&key| self.ctx.geng.window().is_key_pressed(key);
        let input = if self.ctx.assets.config.controls.left.iter().any(is_pressed) {
            Some(Input::Left)
        } else if self.ctx.assets.config.controls.right.iter().any(is_pressed) {
            Some(Input::Right)
        } else if self.ctx.assets.config.controls.skip.iter().any(is_pressed) {
            Some(Input::Skip)
        } else {
            self.touch_input
        };
        let timeline_input = if self.ctx.assets.config.controls.undo.iter().any(is_pressed) {
            Some(-1)
        } else if self.ctx.assets.config.controls.redo.iter().any(is_pressed) {
            Some(1)
        } else {
            None
        };
        let update = self.history_player.update(
            delta_time,
            &self.ctx.assets.logic_config,
            input,
            timeline_input,
        );
        if let Some(moves) = update.started {
            // TODO copypasta
            self.ctx.sound.play_turn_start_sounds(moves);
            self.vfx.add_moves(moves);
        }
        if let Some(moves) = update.finished {
            self.ctx.sound.play_turn_end_sounds(moves);
        }
        if let Some(entity) = self.history_player.frame().current_state.selected_entity() {
            self.camera.center = lerp(
                self.camera.center,
                entity.pos.cell.map(|x| x as f32 + 0.5),
                (delta_time * self.ctx.assets.config.camera_speed).min(1.0),
            );
        }
        if self.history_player.frame().current_state.finished() {
            self.finish(Transition::NextLevel);
        }

        self.vfx.update(delta_time);
        self.ctx
            .sound
            .update_game_tick_time(delta_time / self.ctx.assets.config.animation_time);

        self.next_zzz -= delta_time;
        if self.next_zzz < 0.0 {
            self.zzz = true;
            self.next_zzz += self.ctx.assets.config.animation_time;
            for entity in &self.history_player.frame().current_state.entities {
                if entity.properties.player {
                    self.vfx.zzz(entity.pos.cell + vec2(0, 1));
                }
            }
        }

        ControlFlow::Continue(())
    }
    fn handle_event(&mut self, event: geng::Event) -> ControlFlow<()> {
        // for event in input::Context::handle_event(self, event) {
        //     self.handle_input(event)?;
        // }

        let mut player_input = None;
        match event {
            geng::Event::KeyDown { key } => {
                if key == self.ctx.assets.config.editor.level.controls.toggle {
                    self.finish(Transition::Editor);
                }

                if self.ctx.assets.config.controls.escape.contains(&key) {
                    self.finish(Transition::Exit);
                }

                if let Some(cheats) = &self.ctx.assets.config.controls.cheats {
                    if key == cheats.prev_level {
                        self.finish(Transition::PrevLevel);
                    } else if key == cheats.next_level {
                        self.finish(Transition::NextLevel);
                    }
                }

                if self.ctx.assets.config.controls.restart.contains(&key) {
                    self.history_player.restart();
                }
                if self.ctx.assets.config.controls.undo.contains(&key) {
                    self.history_player.undo();
                }
                if self.ctx.assets.config.controls.redo.contains(&key) {
                    self.history_player.redo();
                }

                if self.ctx.assets.config.controls.left.contains(&key) {
                    player_input = Some(Input::Left);
                }
                if self.ctx.assets.config.controls.right.contains(&key) {
                    player_input = Some(Input::Right);
                }
                if self.ctx.assets.config.controls.skip.contains(&key) {
                    player_input = Some(Input::Skip);
                }
                if self.ctx.assets.config.controls.next_player.contains(&key) {
                    self.history_player
                        .change_player_selection(&self.ctx.assets.logic_config, 1);
                    if let Some(player) =
                        self.history_player.frame().current_state.selected_entity()
                    {
                        self.vfx.change_player(player.pos);
                        self.ctx.sound.player_change();
                    }
                }
                if self.ctx.assets.config.controls.prev_player.contains(&key) {
                    self.history_player
                        .change_player_selection(&self.ctx.assets.logic_config, -1);
                    if let Some(player) =
                        self.history_player.frame().current_state.selected_entity()
                    {
                        self.vfx.change_player(player.pos);
                        self.ctx.sound.player_change();
                    }
                }
            }
            geng::Event::MouseDown { position, .. } => {
                self.click(position)?;
            }
            geng::Event::TouchStart(touch) => {
                if !self.click(touch.position)? {
                    self.touch_input = Some(
                        if (touch.position.x as f32) < self.framebuffer_size.x / 2.0 {
                            Input::Left
                        } else {
                            Input::Right
                        },
                    );
                    player_input = self.touch_input;
                }
            }
            geng::Event::TouchEnd(_touch) => {
                self.touch_input = None;
            }
            _ => {}
        }
        if let Some(input) = player_input {
            self.zzz = false;
            self.next_zzz = self.ctx.assets.config.zzz_time;
            if self.history_player.frame().animation.is_none() {
                if let Some(moves) = self
                    .history_player
                    .process_move(&self.ctx.assets.logic_config, input)
                {
                    self.ctx.sound.play_turn_start_sounds(moves);
                    self.vfx.add_moves(moves);
                }
            }
        }
        ControlFlow::Continue(())
    }
    fn draw(&mut self, framebuffer: &mut ugli::Framebuffer) {
        self.framebuffer_size = framebuffer.size().map(|x| x as f32);
        let frame = self.history_player.frame();
        self.ctx
            .renderer
            .draw(framebuffer, &self.camera, frame, &self.level_mesh, self.zzz);
        self.vfx.draw(framebuffer, &self.camera);
        buttons::layout(
            &mut self.buttons,
            self.ui_camera
                .view_area(self.framebuffer_size)
                .bounding_box(),
        );
        let ui_cursor_pos = self.ui_camera.screen_to_world(
            self.framebuffer_size,
            self.ctx.geng.window().cursor_position().map(|x| x as f32),
        );
        for (matrix, button) in buttons::matrices(ui_cursor_pos, &self.buttons) {
            self.ctx.renderer.draw_tile(
                framebuffer,
                &self.ui_camera,
                match button.button_type {
                    ButtonType::Undo => "Undo",
                    ButtonType::Redo => "Redo",
                    ButtonType::Reset => "Reset",
                    ButtonType::Exit => "Home",
                    ButtonType::SwitchPlayer => "SwitchPlayer", // TODO
                },
                Rgba::WHITE,
                matrix,
            );
        }
    }

    fn click(&mut self, position: vec2<f64>) -> ControlFlow<(), bool> {
        let ui_pos = self
            .ui_camera
            .screen_to_world(self.framebuffer_size, position.map(|x| x as f32));
        if let Some(button) = self
            .buttons
            .iter()
            .find(|button| button.calculated_pos.contains(ui_pos))
        {
            match button.button_type {
                ButtonType::Undo => self.history_player.undo(),
                ButtonType::Redo => self.history_player.redo(),
                ButtonType::Reset => self.history_player.restart(),
                ButtonType::Exit => return ControlFlow::Break(()),
                ButtonType::SwitchPlayer => {
                    self.history_player
                        .change_player_selection(&self.ctx.assets.logic_config, 1);
                    if let Some(player) =
                        self.history_player.frame().current_state.selected_entity()
                    {
                        self.vfx.change_player(player.pos);
                        self.ctx.sound.player_change();
                    }
                }
            }
            return ControlFlow::Continue(true);
        }
        ControlFlow::Continue(false)
    }
}
