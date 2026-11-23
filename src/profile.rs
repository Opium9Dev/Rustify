#[derive(Clone)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub tastes: Vec<usize>,
    pub avatar: usize,
    pub photo: bool,
    pub accent: Option<u32>,
    pub language: String,
}

pub const PHOTO_PX: u32 = 256;
