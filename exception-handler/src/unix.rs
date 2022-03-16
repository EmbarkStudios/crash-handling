mod pthread_intercept;

// Force this function to be linked, but it shouldn't actually be called by
// users directly
#[doc(hidden)]
pub use pthread_intercept::pthread_create;
