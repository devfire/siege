use macroquad::window::Conf;

fn config() -> Conf {
    Conf {
        window_title: "Siege!".into(),
        window_width: 1600,
        window_height: 900,
        ..Default::default()
    }
}

#[macroquad::main(config)]
async fn main() {
    siege::run().await;
}
