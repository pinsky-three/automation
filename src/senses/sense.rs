use std::error::Error;

trait Sense {
    fn sense(&self) -> Result<(), Box<dyn Error>>;
}
