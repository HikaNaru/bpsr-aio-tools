use anyhow::Result;
use enigo::{Enigo, Key, Keyboard, Settings};

pub struct InputController {
    enigo: Enigo,
}

impl InputController {
    pub fn new() -> Result<Self> {
        Ok(Self {
            enigo: Enigo::new(&Settings::default())?,
        })
    }

    pub fn press_key(&mut self, key: Key) -> Result<()> {
        use enigo::Direction;
        self.enigo.key(key, Direction::Click)?;
        Ok(())
    }
}
