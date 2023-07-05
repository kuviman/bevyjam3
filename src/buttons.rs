use super::*;

pub fn matrices<T>(
    cursor_pos: Option<vec2<f32>>,
    buttons: &[Button<T>],
) -> impl Iterator<Item = (mat3<f32>, &Button<T>)> + '_ {
    buttons.iter().map(move |button| {
        let matrix = mat3::translate(button.calculated_pos.bottom_left())
            * mat3::scale(button.calculated_pos.size())
            * mat3::scale_uniform_around(
                vec2::splat(0.5),
                if button.usable
                    && cursor_pos.map_or(false, |cursor_pos| {
                        button.calculated_pos.contains(cursor_pos)
                    })
                {
                    1.1
                } else {
                    1.0
                },
            );
        (matrix, button)
    })
}

pub fn layout<T>(buttons: &mut [Button<T>], viewport: Aabb2<f32>) {
    for button in buttons {
        button.calculated_pos = button
            .pos
            .translate(viewport.bottom_left() + viewport.size() * button.anchor.v());
    }
}

#[derive(Serialize, Deserialize, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Anchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
    TopCenter,
    BottomCenter,
    LeftCenter,
    RightCenter
}

impl Anchor {
    pub fn v(&self) -> vec2<f32> {
        match self {
            Self::TopLeft => vec2(0.0, 1.0),
            Self::TopRight => vec2(1.0, 1.0),
            Self::BottomLeft => vec2(0.0, 0.0),
            Self::BottomRight => vec2(1.0, 0.0),
            Self::Center => vec2(0.5, 0.5),
            Self::TopCenter => vec2(0.5, 1.0),
            Self::BottomCenter => vec2(0.5, 0.0),
            Self::LeftCenter => vec2(0.0, 0.5),
            Self::RightCenter => vec2(1.0, 0.5),
        }
    }
}

pub struct Button<T> {
    pub usable: bool,
    pub anchor: Anchor,
    pub pos: Aabb2<f32>,
    pub calculated_pos: Aabb2<f32>,
    pub button_type: T,
}

impl<T> Button<T> {
    pub fn new(anchor: Anchor, pos: Aabb2<f32>, button_type: T) -> Self {
        Self {
            anchor,
            pos,
            button_type,
            calculated_pos: pos,
            usable: true,
        }
    }

    pub fn square(anchor: Anchor, pos: vec2<f32>, button_type: T) -> Self {
        // TODO configurable?
        let size = 1.0;
        let padding = 0.0;
        Self::new(
            anchor,
            Aabb2::point(pos.map(|x| x as f32 * (size + padding) + padding + size / 2.0))
                .extend_symmetric(vec2::splat(size / 2.0)),
            button_type,
        )
    }
}
