use super::*;

pub fn init(app: &mut App) {
    app.register_ldtk_entity::<GoalBundle>("Goal");
    app.add_systems(
        (finish_level,)
            .in_set(OnUpdate(GameState::Turn))
            .before(end_turn),
    );
}

#[derive(Default, Component)]
struct Goal;

#[derive(Bundle, LdtkEntity)]
struct GoalBundle {
    goal: Goal,
    #[grid_coords]
    position: GridCoords,
    #[sprite_sheet_bundle]
    sprite_sheet: SpriteSheetBundle,
    #[with(entity_name)]
    name: Name,
}

fn finish_level(
    mut level_selection: ResMut<LevelSelection>,
    players: Query<(&GridCoords, &Rotation), With<Player>>,
    goals: Query<&GridCoords, With<Goal>>,
    audio: Res<Audio>,
    asset_server: Res<AssetServer>,
) {
    for (player_coords, player_rotation) in players.iter() {
        if player_rotation.0 != 0 {
            continue;
        }
        for goal_coords in goals.iter() {
            if player_coords == goal_coords {
                audio.play_sfx(asset_server.load("sfx/finishLevel.wav"));
                match *level_selection {
                    LevelSelection::Index(ref mut index) => *index += 1,
                    _ => unreachable!(),
                }
            }
        }
    }
}
