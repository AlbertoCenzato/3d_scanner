use anyhow::Result;
use std::cfg;

pub trait StepperMotor {
    fn step(&mut self, steps: u32);
    fn steps_per_rev(&self) -> f32;
    fn name(&self) -> String;
}

pub fn make_stepper_motor() -> Result<Box<dyn StepperMotor>> {
    #[cfg(feature = "motor")]
    let motor: Box<dyn StepperMotor> = Box::new(real_motor::NemaStepperMotor::new()?);
    #[cfg(not(feature = "motor"))]
    let motor: Box<dyn StepperMotor> = Box::new(MockStepperMotor {});
    return Ok(motor);
}

#[cfg(feature = "motor")]
pub mod real_motor {
    use super::*;
    use rppal::gpio::{Gpio, OutputPin};

    pub struct NemaStepperMotor {
        pin1: OutputPin,
        pin2: OutputPin,
        pin3: OutputPin,
        pin4: OutputPin,
    }

    impl NemaStepperMotor {
        pub fn new() -> rppal::gpio::Result<NemaStepperMotor> {
            let gpio = Gpio::new()?;
            let motor = NemaStepperMotor {
                pin1: gpio.get(17)?.into_output(),
                pin2: gpio.get(27)?.into_output(),
                pin3: gpio.get(22)?.into_output(),
                pin4: gpio.get(23)?.into_output(),
            };
            return Ok(motor);
        }
    }

    impl StepperMotor for NemaStepperMotor {
        fn steps_per_rev(&self) -> f32 {
            return 200_f32;
        }

        fn step(&mut self, steps: u32) {
            use rppal::gpio::Level;

            const SINGLE_PHASE_STEPPING: [[Level; 4]; 4] = [
                [Level::High, Level::Low, Level::Low, Level::Low],
                [Level::Low, Level::Low, Level::High, Level::Low],
                [Level::Low, Level::High, Level::Low, Level::Low],
                [Level::Low, Level::Low, Level::Low, Level::High],
            ];

            const DOUBLE_PHASE_STEPPING: [[Level; 4]; 4] = [
                [Level::High, Level::Low, Level::High, Level::Low],
                [Level::Low, Level::High, Level::High, Level::Low],
                [Level::Low, Level::High, Level::Low, Level::High],
                [Level::High, Level::Low, Level::Low, Level::High],
            ];

            // non funziona
            const HALF_PHASE_STEPPING: [[Level; 4]; 8] = [
                [Level::High, Level::Low, Level::High, Level::Low],
                [Level::Low, Level::Low, Level::High, Level::Low],
                [Level::Low, Level::High, Level::High, Level::Low],
                [Level::Low, Level::High, Level::Low, Level::Low],
                [Level::Low, Level::High, Level::Low, Level::High],
                [Level::Low, Level::Low, Level::Low, Level::High],
                [Level::High, Level::Low, Level::Low, Level::High],
                [Level::High, Level::Low, Level::Low, Level::Low],
            ];

            for step in 0..steps {
                let m = step % 4;
                let sequence = DOUBLE_PHASE_STEPPING[m as usize];
                self.pin1.write(sequence[0]);
                self.pin2.write(sequence[1]);
                self.pin3.write(sequence[2]);
                self.pin4.write(sequence[3]);
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }

        fn name(&self) -> String {
            return "Nema 17".to_string();
        }
    }
}

pub struct MockStepperMotor {}

impl StepperMotor for MockStepperMotor {
    fn steps_per_rev(&self) -> f32 {
        return 0_f32;
    }

    fn step(&mut self, _steps: u32) {}

    fn name(&self) -> String {
        return "Mock Motor".to_string();
    }
}
