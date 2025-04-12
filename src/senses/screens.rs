use xcap::Monitor;

pub struct Screens {
    monitors: Vec<Monitor>,
}

impl Screens {
    pub fn new() -> Self {
        let monitors = Monitor::all().unwrap();

        Self { monitors }
    }

    pub fn get_monitors(&self) -> &Vec<Monitor> {
        &self.monitors
    }

    pub fn report(&self) {
        for (i, monitor) in self.monitors.iter().enumerate() {
            println!("Monitor {}: {}", i, monitor.name().unwrap());
        }
    }
}
