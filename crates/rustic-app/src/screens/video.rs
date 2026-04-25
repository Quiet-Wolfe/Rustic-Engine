#[cfg(not(target_os = "android"))]
#[path = "video_desktop.rs"]
mod video_desktop;
#[cfg(not(target_os = "android"))]
pub use video_desktop::VideoPlayer;

#[cfg(target_os = "android")]
#[path = "video_android.rs"]
mod video_android;
#[cfg(target_os = "android")]
pub use video_android::VideoPlayer;
