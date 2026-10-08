use crate::unique::Error;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::Security::{
    GetTokenInformation, TokenStatistics, TOKEN_QUERY, TOKEN_STATISTICS,
};
use windows::Win32::System::Threading::{CreateMutexW, GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::WindowsAndMessaging::{
    SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG,
};

const APP_ID: &str = "solar-screen-brightness";

#[allow(dead_code)]
pub struct SsbUniqueInstance(OwnedHandle);

impl SsbUniqueInstance {
    pub fn try_acquire() -> Result<Self, Error> {
        Self::acquire_named(APP_ID)
    }

    fn acquire_named(app_id: &str) -> Result<Self, Error> {
        let acquire = || -> windows::core::Result<Option<Self>> {
            unsafe {
                let mut token = HANDLE::default();
                OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)?;
                // Own the token immediately, including on later API failures.
                let token = OwnedHandle::from_raw_handle(token.0 as _);
                let mut data = TOKEN_STATISTICS::default();
                let mut len = 0;
                GetTokenInformation(
                    HANDLE(token.as_raw_handle() as isize),
                    TokenStatistics,
                    Some(&mut data as *mut _ as _),
                    std::mem::size_of_val(&data) as u32,
                    &mut len,
                )?;
                let luid = data.AuthenticationId;
                let name: Vec<u16> = format!("{app_id}-{}-{}", luid.HighPart, luid.LowPart)
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                // Only the named object's existence is needed, not ownership.
                let mutex = CreateMutexW(None, false, PCWSTR(name.as_ptr()))?;
                let already_exists =
                    GetLastError().is_err_and(|e| e.code() == ERROR_ALREADY_EXISTS.to_hresult());
                // CreateMutex returns a valid handle even for an existing object.
                // Close it on rejection, otherwise it would keep the object alive.
                let mutex = OwnedHandle::from_raw_handle(mutex.0 as _);
                Ok((!already_exists).then_some(Self(mutex)))
            }
        };
        match acquire() {
            Ok(Some(instance)) => Ok(instance),
            Ok(None) => Err(Error::AlreadyRunning(ExistingInstance)),
            Err(e) => Err(Error::PlatformError(Box::new(e))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Threading::GetCurrentProcessId;

    #[test]
    fn rejected_duplicates_do_not_keep_the_mutex_alive() {
        let name = format!("ssb-test-{}", unsafe { GetCurrentProcessId() });
        let owner = SsbUniqueInstance::acquire_named(&name).unwrap();
        for _ in 0..16 {
            assert!(matches!(
                SsbUniqueInstance::acquire_named(&name),
                Err(Error::AlreadyRunning(_))
            ));
        }
        drop(owner);
        assert!(SsbUniqueInstance::acquire_named(&name).is_ok());
    }
}

/// Represents an already running instance of Solar Screen Brightness
#[derive(Debug)]
pub struct ExistingInstance;

impl ExistingInstance {
    /// Sends a broadcast message to the existing instance, telling it to maximise its window
    pub fn wakeup(&self) -> anyhow::Result<()> {
        let message = crate::event_watcher::windows::register_open_window_message();
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                message,
                None,
                None,
                SMTO_ABORTIFHUNG,
                1000,
                None,
            )
        };
        Ok(())
    }
}
