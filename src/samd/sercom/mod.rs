use crate::{Interrupt, Runtime, samd::pac::{interrupt, sercom0::RegisterBlock}};

mod i2c;
pub use i2c::{ I2cController, I2cError };

mod spi;
pub use spi::{ SpiController, SpiConfig };

pub trait StaticSercom: Sercom {
    const ID: u8;

    unsafe fn steal() -> Self;
}

pub trait Sercom {
    fn into_dyn(self) -> DynSercom where Self:Sized + 'static {
        DynSercom { regs: self.regs(), interrupt: self.interrupt() }
    }

    fn interrupt(&self) -> &'static Interrupt;

    fn regs(&self) -> &RegisterBlock;

    fn id(&self) -> u8;
}

pub struct DynSercom {
    regs: *const RegisterBlock,
    interrupt: &'static Interrupt,
}

impl Sercom for DynSercom {
    fn interrupt(&self) -> &'static Interrupt {
        self.interrupt
    }

    fn regs(&self) -> &RegisterBlock {
        unsafe { &*self.regs }
    }

    fn id(&self) -> u8 {
        use crate::samd::pac::{SERCOM0, SERCOM1};
        let offset = SERCOM0::PTR as usize;
        let step = SERCOM1::PTR as usize - SERCOM0::PTR as usize;
        ((self.regs as *const _ as usize - offset) / step) as u8
    }
}

macro_rules! instance {
    ($feature:literal, $name:ident, $pac_name:ident, $int:ident, $id:literal) => {
        #[cfg(feature = $feature)]
        pub struct $name(Runtime);

        #[cfg(feature = $feature)]
        static $int: crate::TaskOnly<Interrupt> = crate::TaskOnly::new(Interrupt::new());

        #[cfg(feature = $feature)]
        impl StaticSercom for $name {
            const ID: u8 = $id;

            /// ## Safety
            ///
            /// This must be called from within the runtime and the peripheral must not exist
            /// elsewhere in the program.
            unsafe fn steal() -> Self {
                unsafe { $name(Runtime::steal()) }
            }
        }

        #[cfg(feature = $feature)]
        impl Sercom for $name {
            fn interrupt(&self) -> &'static Interrupt {
                $int.get(self.0)
            }

            fn regs(&self) -> &RegisterBlock {
                unsafe { &*crate::samd::pac::$pac_name::PTR }
            }

            fn id(&self) -> u8 {
                $id
            }
        }

        #[cfg(feature = $feature)]
        #[interrupt]
        fn $pac_name() {
            // Disable all interrupts by writing all bits to INTENCLR
            unsafe { core::ptr::write_volatile(crate::samd::pac::$pac_name::PTR.cast::<u8>().cast_mut().offset(0x14), 0xff); }
            unsafe { $int.get_unchecked().notify() };
        }
    };
}

instance!("sercom0", Sercom0, SERCOM0, SERCOM0_INT, 0);
instance!("sercom1", Sercom1, SERCOM1, SERCOM1_INT, 1);
instance!("sercom2", Sercom2, SERCOM2, SERCOM2_INT, 2);
instance!("sercom3", Sercom3, SERCOM3, SERCOM3_INT, 3);
instance!("sercom4", Sercom4, SERCOM4, SERCOM4_INT, 4);
instance!("sercom5", Sercom5, SERCOM5, SERCOM5_INT, 5);
