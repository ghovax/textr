use nalgebra_glm::IVec2;

struct Cursor {
    position: IVec2,
    blink: bool,
}

impl Cursor {
    pub fn new() -> Self {
        Cursor {
            position: IVec2::zeros(),
            blink: false,
        }
    }
}
