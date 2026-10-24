macro_rules! ids {
    ($($name:ident),*) => {$(
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub u64);

        impl $name {
            #[must_use]
            pub fn next() -> Self {
                static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
                Self(COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
            }

            #[must_use]
            pub const fn as_u64(self) -> u64 {
                self.0
            }

            #[must_use]
            pub const fn from_u64(val: u64) -> Self {
                Self(val)
            }
        }
    )*};
}

ids!(WindowId, WorkspaceId, OutputId);
