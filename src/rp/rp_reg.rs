// Based on Embassy, under MIT OR Apache-2.0

/// Extension trait for PAC regs, adding atomic xor/bitset/bitclear writes.
#[allow(unused)]
pub trait RpReg<T: Copy> {
    fn write_value_set(&self, v: T);
    fn write_value_clear(&self, v: T);
    fn write_xor<R>(&self, f: impl FnOnce(&mut T) -> R) -> R;
    fn write_set<R>(&self, f: impl FnOnce(&mut T) -> R) -> R;
    fn write_clear<R>(&self, f: impl FnOnce(&mut T) -> R) -> R;
}

impl<T: Default + Copy, A: super::pac::common::Write> RpReg<T> for super::pac::common::Reg<T, A> {
    fn write_xor<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let mut val = Default::default();
        let res = f(&mut val);
        unsafe {
            let ptr = (self.as_ptr() as *mut u8).add(0x1000) as *mut T;
            ptr.write_volatile(val);
        }
        res
    }

    fn write_set<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let mut val = Default::default();
        let res = f(&mut val);
        self.write_value_set(val);
        res
    }

    fn write_value_set(&self, v: T) {
        unsafe {
            let ptr = (self.as_ptr() as *mut u8).add(0x2000) as *mut T;
            ptr.write_volatile(v);
        }
    }

    fn write_clear<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let mut val = Default::default();
        let res = f(&mut val);
        self.write_value_clear(val);
        res
    }

    fn write_value_clear(&self, v: T) {
        unsafe {
            let ptr = (self.as_ptr() as *mut u8).add(0x3000) as *mut T;
            ptr.write_volatile(v);
        }
    }
}
