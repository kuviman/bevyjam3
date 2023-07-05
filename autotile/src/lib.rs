use geng::prelude::*;
use image::GenericImageView;

pub struct Tileset {
    pub texture: ugli::Texture,
    pub def: TilesetDef,
}

#[derive(Debug)]
pub struct TilesetDef {
    pub tile_size: vec2<usize>,
    pub tiles: HashMap<String, Tile>,
}

pub trait TileMap {
    type NonEmptyIter<'a>: Iterator<Item = vec2<i32>> + 'a
    where
        Self: 'a;
    fn non_empty_tiles(&self) -> Self::NonEmptyIter<'_>;
    fn get_at(&self, pos: vec2<i32>) -> Option<&str>; // TODO not &str
}

#[derive(Clone, Debug)]
pub struct TexturedTile {
    pub pos: vec2<i32>,
    pub tileset_pos: vec2<usize>,
}

impl TilesetDef {
    pub fn generate_mesh<'a>(
        &'a self,
        tile_map: &'a impl TileMap,
    ) -> impl Iterator<Item = TexturedTile> + 'a {
        tile_map
            .non_empty_tiles()
            .flat_map(|pos| tile_map.get_at(pos).map(move |value| (pos, value)))
            .flat_map(|(pos, value)| {
                let uv = self
                    .tiles
                    .get(value)
                    .expect(&format!("No def for tile type {value:?}"))
                    .tileset_pos(|delta| match tile_map.get_at(pos + delta) {
                        Some(other) => {
                            if other == value {
                                Connection::Same
                            } else {
                                Connection::Different
                            }
                        }
                        None => Connection::Empty,
                    });
                uv.map(|uv| TexturedTile {
                    pos,
                    tileset_pos: uv,
                })
            })
    }
    fn uv_impl(
        &self,
        tileset_pos: vec2<usize>,
        texture_size: vec2<usize>,
        shrink: usize,
    ) -> Aabb2<f32> {
        let mut pixel_uv = Aabb2::point(tileset_pos)
            .extend_positive(vec2::splat(1))
            .map_bounds(|v| v * self.tile_size.map(|x| x + 2));
        pixel_uv.min += vec2::splat(shrink);
        pixel_uv.max -= vec2::splat(shrink);
        let mut result = pixel_uv
            .map_bounds(|v| v.map(|x| x as f32) / texture_size.map(|x| x as f32))
            .map_bounds(|v| vec2(v.x, 1.0 - v.y));
        mem::swap(&mut result.min.y, &mut result.max.y);
        result
    }

    pub fn uv(&self, tileset_pos: vec2<usize>, texture_size: vec2<usize>) -> Aabb2<f32> {
        self.uv_impl(tileset_pos, texture_size, 1)
    }
    pub fn uv_with_border(
        &self,
        tileset_pos: vec2<usize>,
        texture_size: vec2<usize>,
    ) -> Aabb2<f32> {
        self.uv_impl(tileset_pos, texture_size, 0)
    }
}

#[test]
fn test() {
    let (_config, def) = futures::executor::block_on(TilesetDef::load(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/tileset/config.ron"),
    ))
    .unwrap();

    struct Map(HashMap<vec2<i32>, &'static str>);
    impl TileMap for Map {
        type NonEmptyIter<'a> = Box<dyn Iterator<Item = vec2<i32>> + 'a>;
        fn non_empty_tiles(&self) -> Self::NonEmptyIter<'_> {
            Box::new(self.0.keys().copied())
        }
        fn get_at(&self, pos: vec2<i32>) -> Option<&str> {
            self.0.get(&pos).copied()
        }
    }
    eprintln!("{def:#?}");
    let mut map = Map(HashMap::new());
    map.0.insert(vec2(0, 0), "block");
    map.0.insert(vec2(1, 0), "block");
    let mesh: HashMap<vec2<i32>, vec2<usize>> = def
        .generate_mesh(&map)
        .map(|tile| (tile.pos, tile.tileset_pos))
        .collect();
    assert_eq!(
        map.0.keys().collect::<HashSet<_>>(),
        mesh.keys().collect::<HashSet<_>>(),
    );
}

#[derive(Debug)]
pub struct Tile {
    pub rules: Vec<Rule>,
    pub default: Option<vec2<usize>>,
}

impl Tile {
    pub fn tileset_pos(&self, f: impl Fn(vec2<i32>) -> Connection) -> Option<vec2<usize>> {
        let matched_rules = self.rules.iter().filter(|rule| {
            rule.connections
                .iter()
                .all(|(delta, filter)| filter.matches(f(*delta)))
        });
        // let matched_rules = matched_rules.collect::<Vec<_>>();
        matched_rules
            .choose(&mut thread_rng())
            .map(|rule| rule.tileset_pos)
            .or(self.default)
    }
}

#[derive(Debug)]
pub struct Rule {
    connections: HashMap<vec2<i32>, ConnectionFilter>,
    tileset_pos: vec2<usize>,
}

pub enum Connection {
    Empty,
    Same,
    Different,
}

#[derive(Deserialize, Copy, Clone, Debug, PartialEq, Eq)]
pub enum ConnectionFilter {
    Anything,
    Empty,
    NotEmpty,
    Same,
    Different,
}

impl ConnectionFilter {
    pub fn matches(&self, connection: Connection) -> bool {
        match self {
            Self::NotEmpty => !matches!(connection, Connection::Empty),
            Self::Empty => matches!(connection, Connection::Empty),
            Self::Anything => true,
            Self::Same => matches!(connection, Connection::Same),
            Self::Different => !matches!(connection, Connection::Same),
        }
    }
}

type ColorRules = HashMap<Rgba<u8>, Option<ConnectionFilter>>;
static COLOR_RULES: std::sync::OnceLock<ColorRules> = std::sync::OnceLock::new();
fn color_rules() -> &'static ColorRules {
    COLOR_RULES.get_or_init(|| serde_json::from_str(include_str!("color_rules.json")).unwrap())
}

#[test]
fn test_color_rules_parse() {
    assert_eq!(color_rules()[&Rgba::WHITE], Some(ConnectionFilter::Empty));
    assert_eq!(color_rules()[&Rgba::BLACK], None);
}

impl ConnectionFilter {
    fn from_color(color: Rgba<u8>) -> Option<Self> {
        *color_rules()
            .get(&color)
            .expect(&format!("No rule for color {color:?}"))
    }
}

async fn load_rules_from_image(
    path: impl AsRef<std::path::Path>,
    config: &Config,
) -> anyhow::Result<Vec<Rule>> {
    let bytes = file::load_bytes(path).await?;
    let image = image::load_from_memory(&bytes)?;
    let mut result = Vec::new();
    for (x_index, x) in (0..image.width()).step_by(config.tile_size.x).enumerate() {
        for (y_index, y) in (0..image.height()).step_by(config.tile_size.y).enumerate() {
            let tile = image::GenericImageView::view(
                &image,
                x,
                y,
                config.tile_size.x as u32,
                config.tile_size.y as u32,
            );
            let mut connections = HashMap::new();
            for dx in -1..=1 {
                for dy in -1..=1 {
                    let delta = vec2(dx, dy);
                    let pos = delta.zip(config.tile_size).map(|(d, size)| match d {
                        -1 => 0,
                        0 => size / 2,
                        1 => size - 1,
                        _ => unreachable!(),
                    });
                    let image::Rgba([r, g, b, a]) = tile.get_pixel(pos.x as u32, pos.y as u32);
                    let color = Rgba { r, g, b, a };
                    if a == 0 {
                        continue;
                    }
                    if let Some(connection) = ConnectionFilter::from_color(color) {
                        // Invert y because of different coordinate system in geng/image
                        connections.insert(vec2(delta.x, -delta.y), connection);
                    }
                }
            }
            if !connections.is_empty() {
                result.push(Rule {
                    connections,
                    tileset_pos: vec2(x_index, y_index),
                });
            }
        }
    }
    Ok(result)
}

#[derive(Deserialize)]
pub struct Config {
    pub texture: std::path::PathBuf,
    pub tile_size: vec2<usize>,
    pub tiles: HashMap<String, TileConfig>,
}

#[derive(Deserialize)]
pub enum TileConfig {
    AutoTile {
        color_coded_rules: std::path::PathBuf,
        default: Option<vec2<usize>>,
    },
    At(usize, usize),
}

impl TilesetDef {
    pub async fn load(path: impl AsRef<std::path::Path>) -> anyhow::Result<(Config, Self)> {
        let path = path.as_ref();
        let base_path = path.parent().unwrap();
        let config: Config = file::load_detect(path).await?;
        let mut tiles = HashMap::new();
        for (name, tile) in &config.tiles {
            tiles.insert(
                name.clone(),
                match tile {
                    TileConfig::AutoTile {
                        color_coded_rules: path,
                        default,
                    } => {
                        let rules = load_rules_from_image(base_path.join(path), &config).await?;
                        Tile {
                            rules,
                            default: *default,
                        }
                    }
                    TileConfig::At(x, y) => Tile {
                        rules: vec![],
                        default: Some(vec2(*x, *y)),
                    },
                },
            );
        }
        let def = Self {
            tile_size: config.tile_size,
            tiles,
        };
        Ok((config, def))
    }
}

impl geng::asset::Load for Tileset {
    type Options = ();
    fn load(
        manager: &geng::asset::Manager,
        path: &std::path::Path,
        _options: &Self::Options,
    ) -> geng::asset::Future<Self> {
        let manager = manager.to_owned();
        let path = path.to_owned();
        async move {
            let (config, def) = TilesetDef::load(path.join("config.ron")).await?;
            let original_image =
                image::load_from_memory(&file::load_bytes(path.join(config.texture)).await?)?;
            assert!(original_image.width() as usize % config.tile_size.x == 0);
            assert!(original_image.height() as usize % config.tile_size.y == 0);
            let mut expanded_image = image::RgbaImage::new(
                (original_image.width() as usize / config.tile_size.x * (config.tile_size.x + 2))
                    as u32,
                (original_image.height() as usize / config.tile_size.y * (config.tile_size.y + 2))
                    as u32,
            );
            for tile in def.tiles.values() {
                #[derive(Copy, Clone)]
                enum Copy {
                    Left,
                    Middle,
                    Right,
                }
                let mut copy = |tileset_pos: vec2<usize>, area: vec2<Copy>| {
                    let from = |tileset_pos, tile_size, area| {
                        tileset_pos * tile_size
                            + match area {
                                Copy::Left | Copy::Middle => 0,
                                Copy::Right => tile_size - 1,
                            }
                    };
                    let from_x = from(tileset_pos.x, config.tile_size.x, area.x);
                    let from_y = from(tileset_pos.y, config.tile_size.y, area.y);
                    let size = |tile_size, area| match area {
                        Copy::Left | Copy::Right => 1,
                        Copy::Middle => tile_size,
                    };
                    let size_x = size(config.tile_size.x, area.x);
                    let size_y = size(config.tile_size.y, area.y);
                    let to = |tileset_pos, tile_size, area| {
                        tileset_pos * (tile_size + 2)
                            + match area {
                                Copy::Left => 0,
                                Copy::Middle => 1,
                                Copy::Right => tile_size + 1,
                            }
                    };
                    let to_x = to(tileset_pos.x, config.tile_size.x, area.x);
                    let to_y = to(tileset_pos.y, config.tile_size.y, area.y);
                    use image::GenericImage;
                    expanded_image
                        .copy_from(
                            original_image
                                .view(from_x as u32, from_y as u32, size_x as u32, size_y as u32)
                                .deref(),
                            to_x as u32,
                            to_y as u32,
                        )
                        .unwrap();
                };
                if let Some(default) = tile.default {
                    copy(default, vec2::splat(Copy::Middle));
                }
                for rule in &tile.rules {
                    for dx in -1..=1 {
                        for dy in -1..=1 {
                            if let Some(ConnectionFilter::Same) =
                                rule.connections.get(&vec2(dx, dy))
                            {
                                copy(
                                    rule.tileset_pos,
                                    vec2(dx, -dy).map(|x| match x {
                                        -1 => Copy::Left,
                                        0 => Copy::Middle,
                                        1 => Copy::Right,
                                        _ => unreachable!(),
                                    }),
                                );
                            }
                        }
                    }
                }
            }
            let texture = ugli::Texture::from_image_image(manager.ugli(), expanded_image);
            Ok(Self { texture, def })
        }
        .boxed_local()
    }
    const DEFAULT_EXT: Option<&'static str> = None;
}
