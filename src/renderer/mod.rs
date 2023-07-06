use super::*;

pub mod background;
mod spritesheet;
pub mod ui;
mod vfx;

pub use spritesheet::SpriteSheet;
pub use vfx::Vfx;

#[derive(Deserialize)]
struct ShadowConfig {
    offset: vec2<f32>,
    opacity: f32,
    blacklist: HashSet<String>,
}

#[derive(Deserialize)]
struct VignetteConfig {
    color: Rgba<f32>,
    inner_radius: f32,
    outer_radius: f32,
}

#[derive(Deserialize)]
struct EntityConfig {
    background: Option<std::path::PathBuf>,
    foreground: Option<std::path::PathBuf>,
}

struct EntityAssets {
    background: Option<SpriteSheet>,
    foreground: Option<SpriteSheet>,
}

#[derive(Deref)]
struct EntitiesAssets {
    #[deref]
    entities: HashMap<String, EntityAssets>,
}

impl geng::asset::Load for EntitiesAssets {
    type Options = ();
    fn load(
        manager: &geng::asset::Manager,
        path: &std::path::Path,
        _options: &Self::Options,
    ) -> geng::asset::Future<Self> {
        let manager = manager.clone();
        let path = path.to_owned();
        async move {
            let config: HashMap<String, EntityConfig> =
                file::load_detect(path.join("config.toml")).await?;
            Ok(EntitiesAssets {
                entities: future::join_all(config.into_iter().map(|(name, config)| async {
                    Ok::<_, anyhow::Error>((
                        name,
                        EntityAssets {
                            background: future::OptionFuture::from(
                                config
                                    .background
                                    .map(|file| manager.load_with(path.join(file), &default())),
                            )
                            .await
                            .transpose()?,
                            foreground: future::OptionFuture::from(
                                config
                                    .foreground
                                    .map(|file| manager.load_with(path.join(file), &default())),
                            )
                            .await
                            .transpose()?,
                        },
                    ))
                }))
                .await
                .into_iter()
                .collect::<Result<HashMap<_, _>, _>>()?,
            })
        }
        .boxed_local()
    }
    const DEFAULT_EXT: Option<&'static str> = None;
}

#[derive(Deserialize)]
pub struct Config {
    shadow: ShadowConfig,
    vignette: VignetteConfig,
}

#[derive(geng::asset::Load)]
struct Shaders {
    texture: ugli::Program,
    background: ugli::Program,
    vignette: ugli::Program,
}

#[derive(geng::asset::Load)]
pub struct Assets {
    entities: EntitiesAssets,
    shaders: Shaders,
    game: autotile::Tileset,
    ui: autotile::Tileset,
    vfx: vfx::Assets,
    #[load(options(premultiply_alpha = "true"))]
    numbers: ugli::Texture,
    #[load(load_with = "load_group_icons(&manager)")]
    group_icons: HashMap<String, ugli::Texture>,
}

async fn load_group_icons(
    manager: &geng::asset::Manager,
) -> anyhow::Result<HashMap<String, ugli::Texture>> {
    let group_names = levels::load_group_names().await;
    Ok(
        future::join_all(group_names.into_iter().map(|name| async move {
            let texture: ugli::Texture = manager
                .load_with(
                    levels::group_dir(&name).join("group_icon.png"),
                    &geng::asset::TextureOptions {
                        filter: ugli::Filter::Nearest,
                        ..default()
                    },
                )
                .await?;
            Ok::<_, anyhow::Error>((name, texture))
        }))
        .await
        .into_iter()
        .filter_map(Result::ok)
        .collect(),
    )
}

pub struct Renderer {
    geng: Geng,
    assets: Rc<crate::Assets>,
    background: background::State,
    index_meshes: Vec<ugli::VertexBuffer<TilesetVertex>>,
    game_tile_meshes: HashMap<String, ugli::VertexBuffer<TilesetVertex>>,
    ui_tile_meshes: HashMap<String, ugli::VertexBuffer<TilesetVertex>>,
    grid_mesh: ugli::VertexBuffer<TilesetVertex>,
    white_texture: ugli::Texture,
    quad: ugli::VertexBuffer<draw2d::Vertex>,
    timer: Timer,
}

impl Renderer {
    pub fn new(geng: &Geng, assets: &Rc<crate::Assets>) -> Self {
        let create_mesh = |border, uv| {
            ugli::VertexBuffer::new_static(geng.ugli(), {
                let pos = Aabb2::ZERO
                    .extend_positive(vec2::splat(1.0))
                    .extend_symmetric(border);
                let corners = pos.zip(uv).corners();
                [
                    corners[0], corners[1], corners[2], corners[0], corners[2], corners[3],
                ]
                .map(|vec2((pos_x, uv_x), (pos_y, uv_y))| TilesetVertex {
                    a_pos: vec2(pos_x, pos_y),
                    a_uv: vec2(uv_x, uv_y),
                })
                .to_vec()
            })
        };
        let create_tile_meshes = |tileset: &autotile::Tileset| {
            tileset
                .def
                .tiles
                .iter()
                .filter_map(|(name, tile)| {
                    tile.default.map(|tileset_pos| {
                        (
                            name.to_owned(),
                            create_mesh(
                                vec2::ZERO,
                                tileset.def.uv(tileset_pos, tileset.texture.size()),
                            ),
                            // create_mesh(
                            //     vec2::splat(1.0) / tileset.def.tile_size.map(|x| x as f32),
                            //     tileset
                            //         .def
                            //         .uv_with_border(tileset_pos, tileset.texture.size()),
                            // ),
                        )
                    })
                })
                .collect()
        };
        Self {
            geng: geng.clone(),
            assets: assets.clone(),
            background: background::State::new(geng, assets),
            index_meshes: {
                let texture_size = assets.renderer.numbers.size();
                assert!(texture_size.x % texture_size.y == 0);
                let len = texture_size.x / texture_size.y;
                (0..len)
                    .map(|i| {
                        create_mesh(
                            vec2::ZERO,
                            Aabb2::point(vec2(i as f32 / len as f32, 0.0))
                                .extend_positive(vec2(1.0 / len as f32, 1.0)),
                        )
                    })
                    .collect()
            },
            game_tile_meshes: create_tile_meshes(&assets.renderer.game),
            ui_tile_meshes: create_tile_meshes(&assets.renderer.ui),
            white_texture: ugli::Texture::new_with(geng.ugli(), vec2(1, 1), |_| Rgba::WHITE),
            grid_mesh: Self::create_grid_mesh(geng.ugli(), 100, 100),
            quad: ugli::VertexBuffer::new_static(
                geng.ugli(),
                vec![
                    draw2d::Vertex {
                        a_pos: vec2(-1.0, -1.0),
                    },
                    draw2d::Vertex {
                        a_pos: vec2(1.0, -1.0),
                    },
                    draw2d::Vertex {
                        a_pos: vec2(1.0, 1.0),
                    },
                    draw2d::Vertex {
                        a_pos: vec2(-1.0, 1.0),
                    },
                ],
            ),
            timer: Timer::new(),
        }
    }

    fn create_grid_mesh(
        ugli: &Ugli,
        width: usize,
        height: usize,
    ) -> ugli::VertexBuffer<TilesetVertex> {
        let mut data = Vec::with_capacity(((width + 1) + (height + 1)) * 2);
        for x in 0..=width {
            data.push(TilesetVertex {
                a_pos: vec2(x as f32, 0.0),
                a_uv: vec2::ZERO,
            });
            data.push(TilesetVertex {
                a_pos: vec2(x as f32, height as f32),
                a_uv: vec2::ZERO,
            });
        }
        for y in 0..=height {
            data.push(TilesetVertex {
                a_pos: vec2(0.0, y as f32),
                a_uv: vec2::ZERO,
            });
            data.push(TilesetVertex {
                a_pos: vec2(width as f32, y as f32),
                a_uv: vec2::ZERO,
            });
        }
        ugli::VertexBuffer::new_static(ugli, data)
    }

    pub fn draw_grid(
        &self,
        framebuffer: &mut ugli::Framebuffer,
        camera: &impl geng::AbstractCamera2d,
        color: Rgba<f32>,
    ) {
        let bottom_left_world =
            camera.screen_to_world(framebuffer.size().map(|x| x as f32), vec2::ZERO);
        self.draw_mesh_impl(
            framebuffer,
            camera,
            &self.grid_mesh,
            ugli::DrawMode::Lines { line_width: 1.0 },
            &self.white_texture,
            color,
            mat3::translate(bottom_left_world.map(f32::floor)),
        );
    }

    pub fn draw_background(
        &self,
        assets: &background::Assets,
        framebuffer: &mut ugli::Framebuffer,
        camera: &impl geng::AbstractCamera2d,
    ) {
        self.background.draw(assets, framebuffer, camera);
    }

    pub fn draw_vignette(&self, framebuffer: &mut ugli::Framebuffer) {
        ugli::draw(
            framebuffer,
            &self.assets.renderer.shaders.vignette,
            ugli::DrawMode::TriangleFan,
            &self.quad,
            ugli::uniforms! {
                u_color: self.assets.config.render.vignette.color,
                u_inner_radius: self.assets.config.render.vignette.inner_radius,
                u_outer_radius: self.assets.config.render.vignette.outer_radius,
            },
            ugli::DrawParameters {
                blend_mode: Some(ugli::BlendMode::premultiplied_alpha()),
                ..default()
            },
        );
    }

    pub fn draw_level(
        &self,
        background_assets: &background::Assets,
        framebuffer: &mut ugli::Framebuffer,
        camera: &impl geng::AbstractCamera2d,
        level: &Level,
        level_mesh: &LevelMesh,
    ) {
        // TODO not generate game state on every frame
        self.draw(
            background_assets,
            framebuffer,
            camera,
            history::Frame {
                state: &GameState::init(&self.assets.logic_config, level),
                time_since: 0.0,
            },
            level_mesh,
            false,
        );
    }

    pub fn draw_group_icon(
        &self,
        framebuffer: &mut ugli::Framebuffer,
        camera: &impl geng::AbstractCamera2d,
        group: &str,
        color: Rgba<f32>,
        matrix: mat3<f32>,
    ) {
        if let Some(texture) = self.assets.renderer.group_icons.get(group) {
            self.geng.draw2d().draw2d(
                framebuffer,
                camera,
                &draw2d::TexturedQuad::unit_colored(texture, color).transform(
                    matrix
                        * mat3::scale(vec2(texture.size().map(|x| x as f32).aspect(), 1.0))
                        * mat3::scale_uniform_around(vec2::splat(1.0), 0.5),
                ),
            );
        }
    }

    pub fn draw(
        &self,
        background_assets: &background::Assets,
        framebuffer: &mut ugli::Framebuffer,
        camera: &impl geng::AbstractCamera2d,
        frame: history::Frame,
        level_mesh: &LevelMesh,
        zzz: bool,
    ) {
        self.draw_background(background_assets, framebuffer, camera);

        // Shadow
        {
            let color = Rgba::new(0.0, 0.0, 0.0, self.assets.config.render.shadow.opacity);
            let transform = mat3::translate(self.assets.config.render.shadow.offset);
            self.draw_mesh_impl(
                framebuffer,
                camera,
                &level_mesh.shadow,
                ugli::DrawMode::Triangles,
                &self.assets.renderer.game.texture,
                color,
                transform,
            );
            self.draw_impl(
                framebuffer,
                camera,
                frame,
                transform,
                color,
                zzz,
                |framebuffer: &mut ugli::Framebuffer, camera, name: &str, color, transform| {
                    if !self.assets.config.render.shadow.blacklist.contains(name) {
                        self.draw_game_tile(framebuffer, camera, name, color, transform);
                    }
                },
            );
        }

        // Background for entities
        self.draw_impl(
            framebuffer,
            camera,
            frame,
            mat3::identity(),
            Rgba::WHITE,
            zzz,
            |framebuffer: &mut ugli::Framebuffer, camera, name: &str, color, transform| {
                if let Some(sprite_sheet) = self
                    .assets
                    .renderer
                    .entities
                    .get(name)
                    .and_then(|assets| assets.background.as_ref())
                {
                    self.draw_sprite_sheet(
                        sprite_sheet,
                        framebuffer,
                        camera,
                        color,
                        self.timer.elapsed().as_secs_f64() as f32,
                        transform * mat3::translate(vec2::splat(0.5)),
                    )
                }
            },
        );

        // Actual game layer
        self.draw_mesh_impl(
            framebuffer,
            camera,
            &level_mesh.normal,
            ugli::DrawMode::Triangles,
            &self.assets.renderer.game.texture,
            Rgba::WHITE,
            mat3::identity(),
        );
        self.draw_impl(
            framebuffer,
            camera,
            frame,
            mat3::identity(),
            Rgba::WHITE,
            zzz,
            |framebuffer: &mut ugli::Framebuffer, camera, name: &str, color, transform| {
                self.draw_game_tile(framebuffer, camera, name, color, transform);
            },
        );

        // Foreground for entities
        self.draw_impl(
            framebuffer,
            camera,
            frame,
            mat3::identity(),
            Rgba::WHITE,
            zzz,
            |framebuffer: &mut ugli::Framebuffer, camera, name: &str, color, transform| {
                if let Some(sprite_sheet) = self
                    .assets
                    .renderer
                    .entities
                    .get(name)
                    .and_then(|assets| assets.foreground.as_ref())
                {
                    self.draw_sprite_sheet(
                        sprite_sheet,
                        framebuffer,
                        camera,
                        color,
                        self.timer.elapsed().as_secs_f64() as f32,
                        transform * mat3::translate(vec2::splat(0.5)),
                    )
                }
            },
        );
    }

    fn draw_impl<Cam: geng::AbstractCamera2d>(
        &self,
        framebuffer: &mut ugli::Framebuffer,
        camera: &Cam,
        frame: history::Frame<'_>,
        transform: mat3<f32>,
        color: Rgba<f32>,
        zzz: bool,
        draw_game_tile: impl Fn(&mut ugli::Framebuffer, &Cam, &str, Rgba<f32>, mat3<f32>),
    ) {
        for goal in &frame.state.goals {
            draw_game_tile(
                framebuffer,
                camera,
                "Goal",
                color,
                transform
                    * mat3::translate(goal.pos.cell.map(|x| x as f32 + 0.5))
                    * goal.pos.angle.to_matrix()
                    * mat3::translate(vec2::splat(-0.5)),
            );
        }

        for entity in &frame.state.entities {
            let mut animation_time = 1.0;
            let (from, to, t) = match &entity.current_move {
                Some(entity_move) => {
                    if let EntityMoveType::Jump {
                        cells_traveled,
                        jump_force,
                        ..
                    } = entity_move.move_type
                    {
                        animation_time = cells_traveled as f32 / jump_force as f32;
                    }
                    (
                        entity_move.prev_pos,
                        entity_move.new_pos,
                        (frame.state.current_time.as_secs_f32()
                            - entity_move.start_time.as_secs_f32())
                            / (entity_move.end_time.as_secs_f32()
                                - entity_move.start_time.as_secs_f32()),
                    )
                }
                None => (entity.pos, entity.pos, 0.0),
            };

            fn cube_move_transform(
                from: Position,
                to: Position,
                border_radius: f32,
                t: f32,
            ) -> mat3<f32> {
                let from_pos = from.cell.map(|x| x as f32);
                let to_pos = to.cell.map(|x| x as f32);
                if from.angle == to.angle {
                    return mat3::translate(lerp(from_pos, to_pos, t))
                        * mat3::rotate_around(vec2::splat(0.5), to.angle.to_angle());
                }
                let delta_pos = to_pos - from_pos;
                let delta_rot = to.angle.to_angle() - from.angle.to_angle();
                let rotation_origin = vec2::splat(0.5)
                    + from_pos
                    + delta_pos / 2.0
                    + delta_pos.rotate_90() / (delta_rot / 2.0).tan() / 2.0;

                let from_transform = mat3::translate(from_pos)
                    * mat3::rotate_around(vec2::splat(0.5), from.angle.to_angle());

                // Double border radius when doing 180 since there is also border radius on the
                // level geometry now
                let border_radius: f32 = delta_rot.abs().as_degrees() / 90.0 * border_radius;
                let extra_len = (1.0 / ((1.0 - (t - 0.5).abs() * 2.0) * f32::PI / 4.0).cos() - 1.0)
                    * border_radius;

                mat3::rotate_around(rotation_origin, delta_rot * t)
                    * mat3::translate(
                        (rotation_origin - (from_pos + vec2::splat(0.5))).normalize_or_zero()
                            * extra_len,
                    )
                    * from_transform
            }

            let entity_transform = cube_move_transform(
                from,
                to,
                self.assets.config.border_radius_pixels as f32
                    / self.assets.config.cell_pixel_size as f32,
                t,
            );

            // Static entities are cached in level mesh
            if !entity.properties.r#static {
                let mut color = color;
                if entity.properties.player && Some(entity.id) != frame.state.selected_player {
                    color = Rgba::from_vec4(
                        color.to_vec4() * self.assets.config.deselected_player_color.to_vec4(),
                    );
                }
                draw_game_tile(
                    framebuffer,
                    camera,
                    if zzz && entity.identifier == "Player" {
                        "PlayerZzz"
                    } else if entity.properties.player
                        && Some(entity.id) != frame.state.selected_player
                    {
                        "PlayerUnselected"
                    } else {
                        &entity.identifier
                    },
                    color,
                    transform * entity_transform,
                );
            }

            for (side_index, side) in entity.sides.iter().enumerate() {
                if let Some(effect) = &side.effect {
                    draw_game_tile(
                        framebuffer,
                        camera,
                        &format!("{effect:?}SideEffect"),
                        color,
                        transform
                            * entity_transform
                            * mat3::rotate_around(
                                vec2::splat(0.5),
                                Entity::relative_side_angle(side_index).to_angle()
                                    - Angle::from_degrees(90.0),
                            )
                            * mat3::translate(vec2(0.0, 1.0)),
                    );
                }
            }
        }
        for powerup in &frame.state.powerups {
            draw_game_tile(
                framebuffer,
                camera,
                &format!("{:?}Powerup", powerup.effect),
                color,
                transform
                    * mat3::translate(powerup.pos.cell.map(|x| x as f32 + 0.5))
                    * (powerup.pos.angle - IntAngle::DOWN).to_matrix()
                    * mat3::translate(vec2::splat(-0.5)),
            );
        }
    }

    pub fn draw_game_tile(
        &self,
        framebuffer: &mut ugli::Framebuffer,
        camera: &impl geng::AbstractCamera2d,
        name: &str,
        color: Rgba<f32>,
        matrix: mat3<f32>,
    ) {
        let Some(vertex_data) = self.game_tile_meshes.get(name) else {
            log::error!("No data for rendering {name:?}");
            return;
        };
        self.draw_mesh_impl(
            framebuffer,
            camera,
            vertex_data,
            ugli::DrawMode::Triangles,
            &self.assets.renderer.game.texture,
            color,
            matrix,
        );
    }

    pub fn draw_ui_tile(
        &self,
        framebuffer: &mut ugli::Framebuffer,
        camera: &impl geng::AbstractCamera2d,
        name: &str,
        color: Rgba<f32>,
        matrix: mat3<f32>,
    ) {
        let Some(vertex_data) = self.ui_tile_meshes.get(name) else {
            log::error!("No data for rendering {name:?}");
            return;
        };
        self.draw_mesh_impl(
            framebuffer,
            camera,
            vertex_data,
            ugli::DrawMode::Triangles,
            &self.assets.renderer.ui.texture,
            color,
            matrix,
        );
    }

    pub fn draw_index(
        &self,
        framebuffer: &mut ugli::Framebuffer,
        camera: &impl geng::AbstractCamera2d,
        index: usize,
        color: Rgba<f32>,
        matrix: mat3<f32>,
    ) {
        let vertex_data = self
            .index_meshes
            .get(index)
            .or(self.index_meshes.last())
            .unwrap();
        self.draw_mesh_impl(
            framebuffer,
            camera,
            vertex_data,
            ugli::DrawMode::TriangleFan,
            &self.assets.renderer.numbers,
            color,
            matrix,
        );
    }

    fn draw_mesh_impl(
        &self,
        framebuffer: &mut ugli::Framebuffer,
        camera: &impl geng::AbstractCamera2d,
        vertex_data: impl ugli::VertexDataSource,
        mode: ugli::DrawMode,
        texture: &ugli::Texture,
        color: Rgba<f32>,
        matrix: mat3<f32>,
    ) {
        ugli::draw(
            framebuffer,
            &self.assets.renderer.shaders.texture,
            mode,
            vertex_data,
            (
                ugli::uniforms! {
                    u_model_matrix: matrix,
                    u_color: color,
                    u_texture: texture,
                    u_texture_size: texture.size(),
                },
                camera.uniforms(framebuffer.size().map(|x| x as f32)),
            ),
            ugli::DrawParameters {
                blend_mode: Some(ugli::BlendMode::premultiplied_alpha()),
                ..default()
            },
        );
    }

    pub fn draw_lowres(&self, scale: usize, f: impl FnOnce(&mut ugli::Framebuffer)) {
        self.geng.window().with_framebuffer(|framebuffer| {
            let mut texture =
                ugli::Texture::new_uninitialized(self.geng.ugli(), framebuffer.size() / scale);
            texture.set_filter(ugli::Filter::Nearest);
            {
                let mut framebuffer = ugli::Framebuffer::new_color(
                    self.geng.ugli(),
                    ugli::ColorAttachment::Texture(&mut texture),
                );
                f(&mut framebuffer);
            }
            self.geng.draw2d().draw2d(
                framebuffer,
                &geng::PixelPerfectCamera,
                &draw2d::TexturedQuad::new(
                    Aabb2::ZERO.extend_positive(framebuffer.size().map(|x| x as f32)),
                    &texture,
                ),
            )
        });
    }
}

#[derive(ugli::Vertex, Clone)]
struct TilesetVertex {
    a_uv: vec2<f32>,
    a_pos: vec2<f32>,
}

pub struct LevelMesh {
    normal: ugli::VertexBuffer<TilesetVertex>,
    shadow: ugli::VertexBuffer<TilesetVertex>,
}

impl Renderer {
    pub fn level_mesh(&self, level: &Level) -> LevelMesh {
        LevelMesh {
            normal: self.level_mesh_impl(&HashSet::new(), level),
            shadow: self.level_mesh_impl(&self.assets.config.render.shadow.blacklist, level),
        }
    }

    fn level_mesh_impl(
        &self,
        blacklist: &HashSet<String>,
        level: &Level,
    ) -> ugli::VertexBuffer<TilesetVertex> {
        struct TileMap<'a> {
            config: &'a logicsider::Config,
            level: &'a Level,
            blacklist: &'a HashSet<String>,
        }
        impl autotile::TileMap for TileMap<'_> {
            type NonEmptyIter<'a> = Box<dyn Iterator<Item = vec2<i32>> + 'a> where Self:'a ;
            fn non_empty_tiles(&self) -> Self::NonEmptyIter<'_> {
                Box::new(
                    self.level
                        .entities
                        .iter()
                        .filter(|entity| !self.blacklist.contains(&entity.identifier))
                        .filter(|entity| self.config.entities[&entity.identifier].r#static)
                        .map(|entity| entity.pos.cell),
                )
            }

            fn get_at(&self, pos: vec2<i32>) -> Option<&str> {
                self.level
                    .entities
                    .iter()
                    .find(|entity| entity.pos.cell == pos)
                    .map(|entity| entity.identifier.as_str())
                    .filter(|&name| !self.blacklist.contains(name))
            }
        }
        ugli::VertexBuffer::new_static(
            self.geng.ugli(),
            self.assets
                .renderer
                .game
                .def
                .generate_mesh(&TileMap {
                    config: &self.assets.logic_config,
                    level,
                    blacklist,
                })
                .flat_map(|tile| {
                    let tileset = &self.assets.renderer.game;
                    let uv = tileset.def.uv(tile.tileset_pos, tileset.texture.size());
                    let pos = Aabb2::point(tile.pos)
                        .extend_positive(vec2::splat(1))
                        .map(|x| x as f32);
                    // .extend_symmetric(
                    //     vec2::splat(0.5) / tileset.def.tile_size.map(|x| x as f32),
                    // );
                    let corners = pos.zip(uv).corners();
                    [
                        corners[0], corners[1], corners[2], corners[0], corners[2], corners[3],
                    ]
                    .map(|vec2((pos_x, uv_x), (pos_y, uv_y))| TilesetVertex {
                        a_pos: vec2(pos_x, pos_y),
                        a_uv: vec2(uv_x, uv_y),
                    })
                })
                .collect(),
        )
    }
}
