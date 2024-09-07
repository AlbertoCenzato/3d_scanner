use rppal::gpio::{Gpio, OutputPin};

pub struct StepperMotor {
    pin1: OutputPin,
    pin2: OutputPin,
    pin3: OutputPin,
    pin4: OutputPin,
}

impl StepperMotor {
    pub fn new() -> rppal::gpio::Result<StepperMotor> {
        let gpio = Gpio::new()?;
        let motor = StepperMotor {
            pin1: gpio.get(17)?.into_output(),
            pin2: gpio.get(27)?.into_output(),
            pin3: gpio.get(22)?.into_output(),
            pin4: gpio.get(23)?.into_output(),
        };
        return Ok(motor);
    }

    pub fn step(&mut self, steps: u32) {
        use rppal::gpio::Level;

        const STEP_SEQUENCE: [[Level; 4]; 4] = [
            [Level::High, Level::Low, Level::Low, Level::High],
            [Level::High, Level::High, Level::Low, Level::Low],
            [Level::Low, Level::High, Level::High, Level::Low],
            [Level::Low, Level::Low, Level::High, Level::High],
        ];

        for _ in 0..steps {
            for step in STEP_SEQUENCE.iter() {
                self.pin1.write(step[0]);
                self.pin2.write(step[1]);
                self.pin3.write(step[2]);
                self.pin4.write(step[3]);
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
    }
}
