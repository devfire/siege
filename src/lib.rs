pub mod ai;
pub mod audio;
pub mod fallers;
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
        let input = frame_input();
        state.update(dt, &input, &mut audio);
        render::draw(&state, font.as_ref());
        macroquad::window::next_frame().await;
    }
}

/// Poll macroquad into a [`game::FrameInput`] — the only place hardware
/// input is read. The aim cursor is mapped to world space here so the
/// game sim stays macroquad-free.
fn frame_input() -> game::FrameInput {
    use macroquad::input::{
        KeyCode, MouseButton, is_key_pressed, is_mouse_button_pressed, mouse_position, mouse_wheel,
    };
    let (mx, my) = mouse_position();
    game::FrameInput {
        aim: render::screen_to_world(mx, my),
        wheel: mouse_wheel().1,
        click: is_mouse_button_pressed(MouseButton::Left),
        fire: is_key_pressed(KeyCode::Space),
        pause: is_key_pressed(KeyCode::P) || is_key_pressed(KeyCode::Escape),
        restart: is_key_pressed(KeyCode::R),
        restart_seed: fresh_seed(),
    }
}

/// Fresh seed from the platform clock (works native + wasm).
fn fresh_seed() -> u64 {
    let mut x = macroquad::miniquad::date::now().to_bits();
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    x.wrapping_mul(0x2545_F491_4F6C_DD1D)
}
