use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
};

pub struct RwLock<T> {
    inner: spin::RwLock<()>,
    data: UnsafeCell<T>,
}

impl<T> RwLock<T> {
    pub fn new(data: T) -> Self {
        Self {
            inner: spin::RwLock::new(()),
            data: UnsafeCell::new(data),
        }
    }

    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        let guard = self.inner.read();
        RwLockReadGuard {
            _guard: guard,
            data: unsafe { &*self.data.get() },
        }
    }

    pub fn write(&self) -> RwLockWriteGuard<'_, T> {
        let guard = self.inner.write();
        RwLockWriteGuard {
            _guard: guard,
            data: unsafe { &mut *self.data.get() },
        }
    }

    #[allow(clippy::mut_from_ref)]
    pub unsafe fn force_use(&self) -> &mut T {
        unsafe { &mut *self.data.get() }
    }
}

pub struct RwLockReadGuard<'a, T> {
    _guard: spin::RwLockReadGuard<'a, ()>,
    data: &'a T,
}

impl<T> Deref for RwLockReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.data
    }
}

pub struct RwLockWriteGuard<'a, T> {
    _guard: spin::RwLockWriteGuard<'a, ()>,
    data: &'a mut T,
}

impl<T> Deref for RwLockWriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.data
    }
}

impl<T> DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.data
    }
}
