pub mod ai;
pub mod audio;
pub mod game;
pub mod particles;
pub mod physics;
pub mod render;
pub mod rng;
pub mod world;

/// Game loop: capped dt, phase-aware update, storybook draw. The medieval
/// display font falls back to macroquad's default if it fails to load.
pub async fn run() {
    let font = macroquad::text::load_ttf_font_from_bytes(include_bytes!(
        "../assets/MedievalSharp-Regular.ttf"
    ))
    .ok();
    let mut audio = audio::Audio::new().await;
    let mut state = game::GameState::new(rng::Rng::seed(1));
    loop {
        let dt = macroquad::time::get_frame_time().min(0.05);
        state.update(dt, &mut audio);
        render::draw(&state, font.as_ref());
        macroquad::window::next_frame().await;
    }
}
