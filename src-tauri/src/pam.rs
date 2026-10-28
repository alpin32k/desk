//! Linux lock: check the account password with PAM, loaded at run time (no
//! build dependency on libpam). Service "desk" has no file in /etc/pam.d, so
//! PAM uses "other", i.e. the system's common-auth, like screen lockers do.

use std::ffi::{CStr, CString, c_char, c_int, c_void};

const PAM_SUCCESS: c_int = 0;
const PAM_PROMPT_ECHO_OFF: c_int = 1;
const PAM_PROMPT_ECHO_ON: c_int = 2;
const PAM_CONV_ERR: c_int = 19;
const PAM_BUF_ERR: c_int = 5;

#[repr(C)]
struct PamMessage {
    msg_style: c_int,
    msg: *const c_char,
}

#[repr(C)]
struct PamResponse {
    resp: *mut c_char,
    resp_retcode: c_int,
}

#[repr(C)]
struct PamConv {
    conv: extern "C" fn(c_int, *mut *const PamMessage, *mut *mut PamResponse, *mut c_void) -> c_int,
    appdata_ptr: *mut c_void,
}

struct Answers {
    user: CString,
    password: CString,
}

/// PAM asks; we answer the password for hidden prompts and the user name for visible ones.
extern "C" fn converse(
    n: c_int,
    msgs: *mut *const PamMessage,
    out: *mut *mut PamResponse,
    data: *mut c_void,
) -> c_int {
    if n <= 0 || msgs.is_null() || out.is_null() || data.is_null() {
        return PAM_CONV_ERR;
    }
    // SAFETY: PAM passes `n` messages and our Answers as appdata; responses
    // are malloc'd because PAM frees them.
    unsafe {
        let answers = &*(data as *const Answers);
        let replies =
            libc::calloc(n as usize, std::mem::size_of::<PamResponse>()) as *mut PamResponse;
        if replies.is_null() {
            return PAM_BUF_ERR;
        }
        for i in 0..n as usize {
            // Linux-PAM: `msgs` points to an array of pointers.
            let m = &**msgs.add(i);
            let text = match m.msg_style {
                PAM_PROMPT_ECHO_OFF => Some(&answers.password),
                PAM_PROMPT_ECHO_ON => Some(&answers.user),
                _ => None,
            };
            if let Some(t) = text {
                (*replies.add(i)).resp = libc::strdup(t.as_ptr());
            }
        }
        *out = replies;
    }
    PAM_SUCCESS
}

type Start =
    unsafe extern "C" fn(*const c_char, *const c_char, *const PamConv, *mut *mut c_void) -> c_int;
type Call = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;

/// The user name of this process.
fn user_name() -> Option<CString> {
    // SAFETY: getpwuid returns a pointer into static storage or null.
    unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if pw.is_null() {
            return None;
        }
        Some(CStr::from_ptr((*pw).pw_name).to_owned())
    }
}

/// Whether `password` is this user's account password.
pub fn check_password(password: &str) -> Result<(), String> {
    let user = user_name().ok_or(tr!("nie znam nazwy użytkownika", "unknown user name"))?;
    let password = CString::new(password).map_err(|_| tr!("złe hasło", "wrong password"))?;
    // SAFETY: the symbols have the libpam signatures declared above.
    unsafe {
        let lib = libloading::Library::new("libpam.so.0")
            .map_err(|e| tr!(format!("brak PAM: {e}"), format!("no PAM: {e}")))?;
        let start: libloading::Symbol<Start> =
            lib.get(b"pam_start\0").map_err(|e| e.to_string())?;
        let authenticate: libloading::Symbol<Call> =
            lib.get(b"pam_authenticate\0").map_err(|e| e.to_string())?;
        let acct_mgmt: libloading::Symbol<Call> =
            lib.get(b"pam_acct_mgmt\0").map_err(|e| e.to_string())?;
        let end: libloading::Symbol<Call> = lib.get(b"pam_end\0").map_err(|e| e.to_string())?;
        let answers = Box::new(Answers {
            user: user.clone(),
            password,
        });
        let conv = PamConv {
            conv: converse,
            appdata_ptr: &*answers as *const Answers as *mut c_void,
        };
        let mut handle: *mut c_void = std::ptr::null_mut();
        // No /etc/pam.d/desk: PAM uses "other", i.e. the system's common-auth.
        let service = c"desk";
        let mut rc = start(service.as_ptr(), user.as_ptr(), &conv, &mut handle);
        if rc == PAM_SUCCESS {
            rc = authenticate(handle, 0);
            if rc == PAM_SUCCESS {
                rc = acct_mgmt(handle, 0);
            }
        }
        if !handle.is_null() {
            end(handle, rc);
        }
        drop(answers);
        if rc == PAM_SUCCESS {
            Ok(())
        } else {
            Err(tr!("złe hasło", "wrong password").into())
        }
    }
}
