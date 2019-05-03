use std::ffi::CStr;
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::time::{Duration, SystemTime};

use libc;

use crate::constants::*;
use crate::dc_context::dc_context_t;
use crate::dc_log::*;
use crate::dc_loginparam::*;
use crate::dc_tools::*;
use crate::types::*;
use crate::x::*;

#[repr(C)]
pub struct dc_imap_t {
    pub config: Arc<RwLock<ImapConfig>>,
    pub watch: Arc<(Mutex<bool>, Condvar)>,

    pub get_config: dc_get_config_t,
    pub set_config: dc_set_config_t,
    pub precheck_imf: dc_precheck_imf_t,
    pub receive_imf: dc_receive_imf_t,

    session: Arc<Mutex<Option<Session>>>,
}

#[derive(Debug)]
pub enum FolderMeaning {
    Unknown,
    SentObjects,
    Other,
}

pub enum Client {
    Secure(imap::Client<native_tls::TlsStream<std::net::TcpStream>>),
    Insecure(imap::Client<std::net::TcpStream>),
}

pub enum Session {
    Secure(imap::Session<native_tls::TlsStream<std::net::TcpStream>>),
    Insecure(imap::Session<std::net::TcpStream>),
}

impl From<imap::Client<native_tls::TlsStream<std::net::TcpStream>>> for Client {
    fn from(client: imap::Client<native_tls::TlsStream<std::net::TcpStream>>) -> Self {
        Client::Secure(client)
    }
}

impl From<imap::Client<std::net::TcpStream>> for Client {
    fn from(client: imap::Client<std::net::TcpStream>) -> Self {
        Client::Insecure(client)
    }
}

impl From<imap::Session<native_tls::TlsStream<std::net::TcpStream>>> for Session {
    fn from(session: imap::Session<native_tls::TlsStream<std::net::TcpStream>>) -> Self {
        Session::Secure(session)
    }
}

impl From<imap::Session<std::net::TcpStream>> for Session {
    fn from(session: imap::Session<std::net::TcpStream>) -> Self {
        Session::Insecure(session)
    }
}

impl Client {
    pub fn login<U: AsRef<str>, P: AsRef<str>>(
        self,
        username: U,
        password: P,
    ) -> Result<Session, (imap::error::Error, Client)> {
        match self {
            Client::Secure(i) => i
                .login(username, password)
                .map(Into::into)
                .map_err(|(e, c)| (e, c.into())),
            Client::Insecure(i) => i
                .login(username, password)
                .map(Into::into)
                .map_err(|(e, c)| (e, c.into())),
        }
    }
}

impl Session {
    pub fn capabilities(
        &mut self,
    ) -> imap::error::Result<imap::types::ZeroCopy<imap::types::Capabilities>> {
        match self {
            Session::Secure(i) => i.capabilities(),
            Session::Insecure(i) => i.capabilities(),
        }
    }

    pub fn list(
        &mut self,
        reference_name: Option<&str>,
        mailbox_pattern: Option<&str>,
    ) -> imap::error::Result<imap::types::ZeroCopy<Vec<imap::types::Name>>> {
        match self {
            Session::Secure(i) => i.list(reference_name, mailbox_pattern),
            Session::Insecure(i) => i.list(reference_name, mailbox_pattern),
        }
    }
}

pub struct ImapConfig {
    pub addr: Option<String>,
    pub imap_server: Option<String>,
    pub imap_port: Option<usize>,
    pub imap_user: Option<String>,
    pub imap_pw: Option<String>,
    pub server_flags: Option<usize>,
    pub connected: i32,
    pub idle_set_up: i32,
    pub selected_folder: *mut libc::c_char,
    pub selected_folder_needs_expunge: i32,
    pub should_reconnect: i32,
    pub can_idle: i32,
    pub has_xlist: i32,
    pub imap_delimiter: libc::c_char,
    pub watch_folder: *mut libc::c_char,
    pub fetch_type_prefetch: *mut mailimap_fetch_type,
    pub fetch_type_body: *mut mailimap_fetch_type,
    pub fetch_type_flags: *mut mailimap_fetch_type,
    pub log_connect_errors: i32,
    pub skip_log_capabilities: i32,
}

impl Default for ImapConfig {
    fn default() -> Self {
        let mut cfg = ImapConfig {
            addr: None,
            imap_server: None,
            imap_port: None,
            imap_user: None,
            imap_pw: None,
            server_flags: None,
            connected: 0,
            idle_set_up: 0,
            selected_folder: unsafe { calloc(1, 1) as *mut libc::c_char },
            selected_folder_needs_expunge: 0,
            should_reconnect: 0,
            can_idle: 0,
            has_xlist: 0,
            imap_delimiter: 0 as libc::c_char,
            watch_folder: unsafe { calloc(1, 1) as *mut libc::c_char },
            fetch_type_prefetch: unsafe { mailimap_fetch_type_new_fetch_att_list_empty() },
            fetch_type_body: unsafe { mailimap_fetch_type_new_fetch_att_list_empty() },
            fetch_type_flags: unsafe { mailimap_fetch_type_new_fetch_att_list_empty() },
            log_connect_errors: 1,
            skip_log_capabilities: 0,
        };

        unsafe {
            mailimap_fetch_type_new_fetch_att_list_add(
                cfg.fetch_type_prefetch,
                mailimap_fetch_att_new_uid(),
            );
            mailimap_fetch_type_new_fetch_att_list_add(
                cfg.fetch_type_prefetch,
                mailimap_fetch_att_new_envelope(),
            );

            mailimap_fetch_type_new_fetch_att_list_add(
                cfg.fetch_type_body,
                mailimap_fetch_att_new_flags(),
            );
            mailimap_fetch_type_new_fetch_att_list_add(
                cfg.fetch_type_body,
                mailimap_fetch_att_new_body_peek_section(mailimap_section_new(
                    0 as *mut mailimap_section_spec,
                )),
            );
            mailimap_fetch_type_new_fetch_att_list_add(
                cfg.fetch_type_flags,
                mailimap_fetch_att_new_flags(),
            );
        }
        cfg
    }
}

pub fn dc_imap_new(
    get_config: dc_get_config_t,
    set_config: dc_set_config_t,
    precheck_imf: dc_precheck_imf_t,
    receive_imf: dc_receive_imf_t,
) -> dc_imap_t {
    dc_imap_t::new(get_config, set_config, precheck_imf, receive_imf)
}

impl dc_imap_t {
    pub fn new(
        get_config: dc_get_config_t,
        set_config: dc_set_config_t,
        precheck_imf: dc_precheck_imf_t,
        receive_imf: dc_receive_imf_t,
    ) -> Self {
        dc_imap_t {
            session: Arc::new(Mutex::new(None)),
            config: Arc::new(RwLock::new(ImapConfig::default())),
            watch: Arc::new((Mutex::new(false), Condvar::new())),
            get_config,
            set_config,
            precheck_imf,
            receive_imf,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.session.lock().unwrap().is_some()
    }

    pub fn should_reconnect(&self) -> bool {
        unimplemented!();
    }

    pub fn connect(&self, context: &dc_context_t, lp: *const dc_loginparam_t) -> libc::c_int {
        if lp.is_null() {
            return 0;
        }
        let lp = unsafe { *lp };
        if lp.mail_server.is_null() || lp.mail_user.is_null() || lp.mail_pw.is_null() {
            return 0;
        }

        if self.is_connected() {
            return 1;
        }

        let addr = to_str(lp.addr);
        let imap_server = to_str(lp.mail_server);
        let imap_port = lp.mail_port as u16;
        let imap_user = to_str(lp.mail_user);
        let imap_pw = to_str(lp.mail_pw);
        let server_flags = lp.server_flags as usize;

        let connection_res: imap::error::Result<Client> =
            if (server_flags & (DC_LP_IMAP_SOCKET_STARTTLS | DC_LP_IMAP_SOCKET_PLAIN)) != 0 {
                println!("insecure");
                imap::connect_insecure((imap_server, imap_port)).and_then(|client| {
                    if (server_flags & DC_LP_IMAP_SOCKET_STARTTLS) != 0 {
                        let tls = native_tls::TlsConnector::builder().build().unwrap();
                        client.secure(imap_server, &tls).map(Into::into)
                    } else {
                        Ok(client.into())
                    }
                })
            } else {
                println!("secure: {}:{} - {}", imap_server, imap_port, imap_server);
                let tls = native_tls::TlsConnector::builder()
                    // FIXME: unfortunately this is needed to make things work on macos + testrun.org
                    .danger_accept_invalid_hostnames(true)
                    .build()
                    .unwrap();
                imap::connect((imap_server, imap_port), imap_server, &tls).map(Into::into)
            };

        match connection_res {
            Ok(client) => {
                println!("imap: connected - {} - {}", imap_user, imap_pw);
                // TODO: handle oauth2
                match client.login(imap_user, imap_pw) {
                    Ok(mut session) => {
                        println!("imap: logged in");
                        // TODO: error handling
                        let caps = session.capabilities().unwrap();
                        let can_idle = caps.has("IDLE");
                        let has_xlist = caps.has("XLIST");

                        let caps_list = caps.iter().fold(String::new(), |mut s, c| {
                            s += " ";
                            s += c;
                            s
                        });
                        let caps_list_c = std::ffi::CString::new(caps_list).unwrap();

                        unsafe {
                            dc_log_info(
                                context,
                                0,
                                b"IMAP-capabilities:%s\x00" as *const u8 as *const libc::c_char,
                                caps_list_c.as_ptr(),
                            )
                        };

                        let mut config = self.config.write().unwrap();
                        config.can_idle = can_idle as i32;
                        config.has_xlist = has_xlist as i32;

                        *self.session.lock().unwrap() = Some(session);

                        1
                    }
                    Err((err, _)) => {
                        eprintln!("failed to login: {:?}", err);

                        unsafe {
                            dc_log_event_seq(
                                context,
                                Event::ERROR_NETWORK,
                                &mut 0 as *mut i32,
                                b"Cannot login\x00" as *const u8 as *const libc::c_char,
                            )
                        };

                        0
                    }
                }
            }
            Err(err) => {
                eprintln!("failed to connect: {:?}", err);
                unsafe {
                    dc_log_event_seq(
                        context,
                        Event::ERROR_NETWORK,
                        &mut 0 as *mut i32,
                        b"Could not connect to IMAP-server %s:%i.\x00" as *const u8
                            as *const libc::c_char,
                        imap_server,
                        imap_port as usize as libc::c_int,
                    )
                };

                0
            }
        }
    }

    pub fn disconnect(&self, context: &dc_context_t) {
        // unimplemented!();
        println!("disconnecting");
    }

    // unsafe fn get_error_msg(
    //     context: &dc_context_t,
    //     imap: &dc_imap_t,
    //     what_failed: *const libc::c_char,
    //     code: libc::c_int,
    // ) -> *mut libc::c_char {
    //     let mut stock: *mut libc::c_char = 0 as *mut libc::c_char;
    //     let mut msg: dc_strbuilder_t = dc_strbuilder_t {
    //         buf: 0 as *mut libc::c_char,
    //         allocated: 0,
    //         free: 0,
    //         eos: 0 as *mut libc::c_char,
    //     };
    //     dc_strbuilder_init(&mut msg, 1000);
    //     match code {
    //         28 => {
    //             stock = dc_stock_str_repl_string(context, 60, imap.imap_user);
    //             dc_strbuilder_cat(&mut msg, stock);
    //         }
    //         _ => {
    //             dc_strbuilder_catf(
    //                 &mut msg as *mut dc_strbuilder_t,
    //                 b"%s, IMAP-error #%i\x00" as *const u8 as *const libc::c_char,
    //                 what_failed,
    //                 code,
    //             );
    //         }
    //     }
    //     free(stock as *mut libc::c_void);
    //     stock = 0 as *mut libc::c_char;
    //     if !(*imap.etpan).imap_response.is_null() {
    //         dc_strbuilder_cat(&mut msg, b"\n\n\x00" as *const u8 as *const libc::c_char);
    //         stock = dc_stock_str_repl_string2(
    //             context,
    //             61,
    //             imap.imap_server,
    //             (*imap.etpan).imap_response,
    //         );
    //         dc_strbuilder_cat(&mut msg, stock);
    //     }
    //     free(stock as *mut libc::c_void);
    //     stock = 0 as *mut libc::c_char;
    //     msg.buf
    // }

    // pub unsafe fn dc_imap_is_error(
    //     context: &dc_context_t,
    //     imap: &dc_imap_t,
    //     code: libc::c_int,
    // ) -> libc::c_int {
    //     if code == MAILIMAP_NO_ERROR as libc::c_int
    //         || code == MAILIMAP_NO_ERROR_AUTHENTICATED as libc::c_int
    //         || code == MAILIMAP_NO_ERROR_NON_AUTHENTICATED as libc::c_int
    //     {
    //         return 0;
    //     }
    //     if code == MAILIMAP_ERROR_STREAM as libc::c_int
    //         || code == MAILIMAP_ERROR_PARSE as libc::c_int
    //     {
    //         dc_log_info(
    //             context,
    //             0,
    //             b"IMAP stream lost; we\'ll reconnect soon.\x00" as *const u8 as *const libc::c_char,
    //         );
    //         imap.should_reconnect = 1
    //     }
    //     1
    // }

    pub fn set_watch_folder(&self, watch_folder: *const libc::c_char) {
        unimplemented!()
    }

    // pub unsafe extern "C" fn dc_imap_set_watch_folder(
    //     imap: &dc_imap_t,
    //     watch_folder: *const libc::c_char,
    // ) {
    //     if watch_folder.is_null() {
    //         return;
    //     }
    //     free(imap.watch_folder as *mut libc::c_void);
    //     imap.watch_folder = dc_strdup(watch_folder);
    // }

    // pub unsafe fn dc_imap_is_connected(imap: &dc_imap_t) -> libc::c_int {
    //     (0 != imap.connected) as libc::c_int
    // }

    pub fn fetch(&self, context: &dc_context_t) -> libc::c_int {
        unimplemented!()
    }

    // pub unsafe fn dc_imap_fetch(context: &dc_context_t, imap: &dc_imap_t) -> libc::c_int {
    //     println!("dc_imap_fetch");
    //     let mut success = 0;
    //     if 0 != imap.connected {
    //         setup_handle_if_needed(context, imap);
    //         let mut cnt = fetch_from_single_folder(context, imap, imap.watch_folder);
    //         while cnt > 0 {
    //             println!("  -- fetching {}", cnt);
    //             cnt = fetch_from_single_folder(context, imap, imap.watch_folder);
    //         }
    //         success = 1;
    //     }

    //     println!("dc_imap_fetch done {}", success);
    //     success
    // }

    pub fn fetch_from_single_folder(
        &self,
        context: &dc_context_t,
        folder: *const libc::c_char,
    ) -> libc::c_int {
        unimplemented!()
    }

    // unsafe fn fetch_from_single_folder(
    //     context: &dc_context_t,
    //     imap: &dc_imap_t,
    //     folder: *const libc::c_char,
    // ) -> libc::c_int {
    //     let mut current_block: u64;
    //     let mut r: libc::c_int = 0;
    //     let mut uidvalidity: uint32_t = 0 as uint32_t;
    //     let mut lastseenuid: uint32_t = 0 as uint32_t;
    //     let mut new_lastseenuid: uint32_t = 0 as uint32_t;
    //     let mut fetch_result: *mut clist = 0 as *mut clist;
    //     let mut read_cnt: size_t = 0 as size_t;
    //     let mut read_errors: size_t = 0 as size_t;
    //     let mut cur: *mut clistiter = 0 as *mut clistiter;
    //     let mut set: *mut mailimap_set = 0 as *mut mailimap_set;
    //     if imap.etpan.is_null() {
    //         dc_log_info(
    //             context,
    //             0,
    //             b"Cannot fetch from \"%s\" - not connected.\x00" as *const u8
    //                 as *const libc::c_char,
    //             folder,
    //         );
    //     } else if select_folder(context, imap, folder) == 0 {
    //         dc_log_warning(
    //             context,
    //             0,
    //             b"Cannot select folder %s for fetching.\x00" as *const u8 as *const libc::c_char,
    //             folder,
    //         );
    //     } else {
    //         println!(
    //             "fetching from folder {}",
    //             std::ffi::CStr::from_ptr(folder).to_str().unwrap()
    //         );
    //         get_config_lastseenuid(context, imap, folder, &mut uidvalidity, &mut lastseenuid);
    //         if uidvalidity != (*(*imap.etpan).imap_selection_info).sel_uidvalidity {
    //             /* first time this folder is selected or UIDVALIDITY has changed, init lastseenuid and save it to config */
    //             if (*(*imap.etpan).imap_selection_info).sel_uidvalidity <= 0 as libc::c_uint {
    //                 dc_log_error(
    //                     context,
    //                     0,
    //                     b"Cannot get UIDVALIDITY for folder \"%s\".\x00" as *const u8
    //                         as *const libc::c_char,
    //                     folder,
    //                 );
    //                 current_block = 17288151659885296046;
    //             } else {
    //                 if 0 != (*(*imap.etpan).imap_selection_info).sel_has_exists() {
    //                     if (*(*imap.etpan).imap_selection_info).sel_exists <= 0 as libc::c_uint {
    //                         dc_log_info(
    //                             context,
    //                             0,
    //                             b"Folder \"%s\" is empty.\x00" as *const u8 as *const libc::c_char,
    //                             folder,
    //                         );
    //                         if (*(*imap.etpan).imap_selection_info).sel_exists == 0 as libc::c_uint
    //                         {
    //                             set_config_lastseenuid(
    //                                 context,
    //                                 imap,
    //                                 folder,
    //                                 (*(*imap.etpan).imap_selection_info).sel_uidvalidity,
    //                                 0 as uint32_t,
    //                             );
    //                         }
    //                         current_block = 17288151659885296046;
    //                     } else {
    //                         set = mailimap_set_new_single(
    //                             (*(*imap.etpan).imap_selection_info).sel_exists,
    //                         );
    //                         current_block = 11057878835866523405;
    //                     }
    //                 } else {
    //                     dc_log_info(
    //                         context,
    //                         0,
    //                         b"EXISTS is missing for folder \"%s\", using fallback.\x00" as *const u8
    //                             as *const libc::c_char,
    //                         folder,
    //                     );
    //                     set = mailimap_set_new_single(0 as uint32_t);
    //                     current_block = 11057878835866523405;
    //                 }
    //                 match current_block {
    //                     17288151659885296046 => {}
    //                     _ => {
    //                         r = mailimap_fetch(
    //                             imap.etpan,
    //                             set,
    //                             imap.fetch_type_prefetch,
    //                             &mut fetch_result,
    //                         );
    //                         if !set.is_null() {
    //                             mailimap_set_free(set);
    //                             set = 0 as *mut mailimap_set
    //                         }
    //                         if 0 != dc_imap_is_error(context, imap, r) || fetch_result.is_null() {
    //                             fetch_result = 0 as *mut clist;
    //                             dc_log_info(
    //                                 context,
    //                                 0,
    //                                 b"No result returned for folder \"%s\".\x00" as *const u8
    //                                     as *const libc::c_char,
    //                                 folder,
    //                             );
    //                             /* this might happen if the mailbox is empty an EXISTS does not work */
    //                             current_block = 17288151659885296046;
    //                         } else {
    //                             cur = (*fetch_result).first;
    //                             if cur.is_null() {
    //                                 dc_log_info(
    //                                     context,
    //                                     0,
    //                                     b"Empty result returned for folder \"%s\".\x00" as *const u8
    //                                         as *const libc::c_char,
    //                                     folder,
    //                                 );
    //                                 /* this might happen if the mailbox is empty an EXISTS does not work */
    //                                 current_block = 17288151659885296046;
    //                             } else {
    //                                 let mut msg_att: *mut mailimap_msg_att = (if !cur.is_null() {
    //                                     (*cur).data
    //                                 } else {
    //                                     0 as *mut libc::c_void
    //                                 })
    //                                     as *mut mailimap_msg_att;
    //                                 lastseenuid = peek_uid(msg_att);
    //                                 if !fetch_result.is_null() {
    //                                     mailimap_fetch_list_free(fetch_result);
    //                                     fetch_result = 0 as *mut clist
    //                                 }
    //                                 if lastseenuid <= 0 as libc::c_uint {
    //                                     dc_log_error(
    //                                         context,
    //                                         0,
    //                                         b"Cannot get largest UID for folder \"%s\"\x00"
    //                                             as *const u8
    //                                             as *const libc::c_char,
    //                                         folder,
    //                                     );
    //                                     current_block = 17288151659885296046;
    //                                 } else {
    //                                     if uidvalidity > 0 as libc::c_uint
    //                                         && lastseenuid > 1 as libc::c_uint
    //                                     {
    //                                         lastseenuid = (lastseenuid as libc::c_uint)
    //                                             .wrapping_sub(1 as libc::c_uint)
    //                                             as uint32_t
    //                                             as uint32_t
    //                                     }
    //                                     uidvalidity =
    //                                         (*(*imap.etpan).imap_selection_info).sel_uidvalidity;
    //                                     set_config_lastseenuid(
    //                                         context,
    //                                         imap,
    //                                         folder,
    //                                         uidvalidity,
    //                                         lastseenuid,
    //                                     );
    //                                     dc_log_info(
    //                                         context,
    //                                         0,
    //                                         b"lastseenuid initialized to %i for %s@%i\x00"
    //                                             as *const u8
    //                                             as *const libc::c_char,
    //                                         lastseenuid as libc::c_int,
    //                                         folder,
    //                                         uidvalidity as libc::c_int,
    //                                     );
    //                                     current_block = 2516253395664191498;
    //                                 }
    //                             }
    //                         }
    //                     }
    //                 }
    //             }
    //         } else {
    //             current_block = 2516253395664191498;
    //         }
    //         match current_block {
    //             17288151659885296046 => {}
    //             _ => {
    //                 set = mailimap_set_new_interval(
    //                     lastseenuid.wrapping_add(1 as libc::c_uint),
    //                     0 as uint32_t,
    //                 );
    //                 r = mailimap_uid_fetch(
    //                     imap.etpan,
    //                     set,
    //                     imap.fetch_type_prefetch,
    //                     &mut fetch_result,
    //                 );
    //                 if !set.is_null() {
    //                     mailimap_set_free(set);
    //                     set = 0 as *mut mailimap_set
    //                 }
    //                 if 0 != dc_imap_is_error(context, imap, r) || fetch_result.is_null() {
    //                     fetch_result = 0 as *mut clist;
    //                     if r == MAILIMAP_ERROR_PROTOCOL as libc::c_int {
    //                         dc_log_info(
    //                             context,
    //                             0,
    //                             b"Folder \"%s\" is empty\x00" as *const u8 as *const libc::c_char,
    //                             folder,
    //                         );
    //                     } else {
    //                         /* the folder is simply empty, this is no error */
    //                         dc_log_warning(
    //                             context,
    //                             0,
    //                             b"Cannot fetch message list from folder \"%s\".\x00" as *const u8
    //                                 as *const libc::c_char,
    //                             folder,
    //                         );
    //                     }
    //                 } else {
    //                     cur = (*fetch_result).first;
    //                     while !cur.is_null() {
    //                         let mut msg_att_0: *mut mailimap_msg_att = (if !cur.is_null() {
    //                             (*cur).data
    //                         } else {
    //                             0 as *mut libc::c_void
    //                         })
    //                             as *mut mailimap_msg_att;
    //                         let mut cur_uid: uint32_t = peek_uid(msg_att_0);
    //                         if cur_uid > lastseenuid {
    //                             let mut rfc724_mid: *mut libc::c_char =
    //                                 unquote_rfc724_mid(peek_rfc724_mid(msg_att_0));
    //                             read_cnt = read_cnt.wrapping_add(1);
    //                             if 0 == imap.precheck_imf.expect("non-null function pointer")(
    //                                 context, rfc724_mid, folder, cur_uid,
    //                             ) {
    //                                 if fetch_single_msg(context, imap, folder, cur_uid) == 0 {
    //                                     dc_log_info(context, 0,
    //                                             b"Read error for message %s from \"%s\", trying over later.\x00"
    //                                             as *const u8 as
    //                                             *const libc::c_char,
    //                                             rfc724_mid, folder);
    //                                     read_errors = read_errors.wrapping_add(1)
    //                                 }
    //                             } else {
    //                                 dc_log_info(
    //                                     context,
    //                                     0,
    //                                     b"Skipping message %s from \"%s\" by precheck.\x00"
    //                                         as *const u8
    //                                         as *const libc::c_char,
    //                                     rfc724_mid,
    //                                     folder,
    //                                 );
    //                             }
    //                             if cur_uid > new_lastseenuid {
    //                                 new_lastseenuid = cur_uid
    //                             }
    //                             free(rfc724_mid as *mut libc::c_void);
    //                         }
    //                         cur = if !cur.is_null() {
    //                             (*cur).next
    //                         } else {
    //                             0 as *mut clistcell_s
    //                         }
    //                     }
    //                     if 0 == read_errors && new_lastseenuid > 0 as libc::c_uint {
    //                         set_config_lastseenuid(
    //                             context,
    //                             imap,
    //                             folder,
    //                             uidvalidity,
    //                             new_lastseenuid,
    //                         );
    //                     }
    //                 }
    //             }
    //         }
    //     }

    //     /* done */
    //     if 0 != read_errors {
    //         dc_log_warning(
    //             context,
    //             0,
    //             b"%i mails read from \"%s\" with %i errors.\x00" as *const u8
    //                 as *const libc::c_char,
    //             read_cnt as libc::c_int,
    //             folder,
    //             read_errors as libc::c_int,
    //         );
    //     } else {
    //         dc_log_info(
    //             context,
    //             0i32,
    //             b"%i mails read from \"%s\".\x00" as *const u8 as *const libc::c_char,
    //             read_cnt as libc::c_int,
    //             folder,
    //         );
    //     }
    //     if !fetch_result.is_null() {
    //         mailimap_fetch_list_free(fetch_result);
    //         fetch_result = 0 as *mut clist
    //     }

    //     read_cnt as libc::c_int
    // }

    // unsafe fn set_config_lastseenuid(
    //     context: &dc_context_t,
    //     imap: &dc_imap_t,
    //     folder: *const libc::c_char,
    //     uidvalidity: uint32_t,
    //     lastseenuid: uint32_t,
    // ) {
    //     let mut key: *mut libc::c_char = dc_mprintf(
    //         b"imap.mailbox.%s\x00" as *const u8 as *const libc::c_char,
    //         folder,
    //     );
    //     let mut val: *mut libc::c_char = dc_mprintf(
    //         b"%lu:%lu\x00" as *const u8 as *const libc::c_char,
    //         uidvalidity,
    //         lastseenuid,
    //     );
    //     imap.set_config.expect("non-null function pointer")(context, key, val);
    //     free(val as *mut libc::c_void);
    //     free(key as *mut libc::c_void);
    // }

    // unsafe fn peek_rfc724_mid(msg_att: *mut mailimap_msg_att) -> *const libc::c_char {
    //     if msg_att.is_null() {
    //         return 0 as *const libc::c_char;
    //     }
    //     /* search the UID in a list of attributes returned by a FETCH command */
    //     let mut iter1: *mut clistiter = 0 as *mut clistiter;
    //     iter1 = (*(*msg_att).att_list).first;
    //     while !iter1.is_null() {
    //         let mut item: *mut mailimap_msg_att_item = (if !iter1.is_null() {
    //             (*iter1).data
    //         } else {
    //             0 as *mut libc::c_void
    //         }) as *mut mailimap_msg_att_item;
    //         if !item.is_null() {
    //             if (*item).att_type == MAILIMAP_MSG_ATT_ITEM_STATIC as libc::c_int {
    //                 if (*(*item).att_data.att_static).att_type
    //                     == MAILIMAP_MSG_ATT_ENVELOPE as libc::c_int
    //                 {
    //                     let mut env: *mut mailimap_envelope =
    //                         (*(*item).att_data.att_static).att_data.att_env;
    //                     if !env.is_null() && !(*env).env_message_id.is_null() {
    //                         return (*env).env_message_id;
    //                     }
    //                 }
    //             }
    //         }
    //         iter1 = if !iter1.is_null() {
    //             (*iter1).next
    //         } else {
    //             0 as *mut clistcell_s
    //         }
    //     }
    //     return 0 as *const libc::c_char;
    // }

    // unsafe fn unquote_rfc724_mid(in_0: *const libc::c_char) -> *mut libc::c_char {
    //     /* remove < and > from the given message id */
    //     let mut out: *mut libc::c_char = dc_strdup(in_0);
    //     let mut out_len: libc::c_int = strlen(out) as libc::c_int;
    //     if out_len > 2 {
    //         if *out.offset(0isize) as libc::c_int == '<' as i32 {
    //             *out.offset(0isize) = ' ' as i32 as libc::c_char
    //         }
    //         if *out.offset((out_len - 1) as isize) as libc::c_int == '>' as i32 {
    //             *out.offset((out_len - 1) as isize) = ' ' as i32 as libc::c_char
    //         }
    //         dc_trim(out);
    //     }
    //     out
    // }

    // /* ******************************************************************************
    //  * Fetch Messages
    //  ******************************************************************************/
    // unsafe fn peek_uid(msg_att: *mut mailimap_msg_att) -> uint32_t {
    //     /* search the UID in a list of attributes returned by a FETCH command */
    //     let mut iter1: *mut clistiter = 0 as *mut clistiter;
    //     iter1 = (*(*msg_att).att_list).first;
    //     while !iter1.is_null() {
    //         let mut item: *mut mailimap_msg_att_item = (if !iter1.is_null() {
    //             (*iter1).data
    //         } else {
    //             0 as *mut libc::c_void
    //         }) as *mut mailimap_msg_att_item;
    //         if !item.is_null() {
    //             if (*item).att_type == MAILIMAP_MSG_ATT_ITEM_STATIC as libc::c_int {
    //                 if (*(*item).att_data.att_static).att_type
    //                     == MAILIMAP_MSG_ATT_UID as libc::c_int
    //                 {
    //                     return (*(*item).att_data.att_static).att_data.att_uid;
    //                 }
    //             }
    //         }
    //         iter1 = if !iter1.is_null() {
    //             (*iter1).next
    //         } else {
    //             0 as *mut clistcell_s
    //         }
    //     }
    //     0 as uint32_t
    // }

    // unsafe fn fetch_single_msg(
    //     context: &dc_context_t,
    //     imap: &dc_imap_t,
    //     folder: *const libc::c_char,
    //     server_uid: uint32_t,
    // ) -> libc::c_int {
    //     let mut msg_att: *mut mailimap_msg_att = 0 as *mut mailimap_msg_att;
    //     /* the function returns:
    //         0  the caller should try over again later
    //     or  1  if the messages should be treated as received, the caller should not try to read the message again (even if no database entries are returned) */
    //     let mut msg_content: *mut libc::c_char = 0 as *mut libc::c_char;
    //     let mut msg_bytes: size_t = 0 as size_t;
    //     let mut r: libc::c_int = 0;
    //     let mut retry_later: libc::c_int = 0;
    //     let mut deleted: libc::c_int = 0;
    //     let mut flags: uint32_t = 0 as uint32_t;
    //     let mut fetch_result: *mut clist = 0 as *mut clist;
    //     let mut cur: *mut clistiter = 0 as *mut clistiter;
    //     if !imap.etpan.is_null() {
    //         let mut set: *mut mailimap_set = mailimap_set_new_single(server_uid);
    //         r = mailimap_uid_fetch(imap.etpan, set, imap.fetch_type_body, &mut fetch_result);
    //         if !set.is_null() {
    //             mailimap_set_free(set);
    //             set = 0 as *mut mailimap_set
    //         }
    //         if 0 != dc_imap_is_error(context, imap, r) || fetch_result.is_null() {
    //             fetch_result = 0 as *mut clist;
    //             dc_log_warning(
    //                 context,
    //                 0,
    //                 b"Error #%i on fetching message #%i from folder \"%s\"; retry=%i.\x00"
    //                     as *const u8 as *const libc::c_char,
    //                 r as libc::c_int,
    //                 server_uid as libc::c_int,
    //                 folder,
    //                 imap.should_reconnect as libc::c_int,
    //             );
    //             if 0 != imap.should_reconnect {
    //                 retry_later = 1
    //             }
    //         } else {
    //             /* this is an error that should be recovered; the caller should try over later to fetch the message again (if there is no such message, we simply get an empty result) */
    //             cur = (*fetch_result).first;
    //             if cur.is_null() {
    //                 dc_log_warning(
    //                     context,
    //                     0,
    //                     b"Message #%i does not exist in folder \"%s\".\x00" as *const u8
    //                         as *const libc::c_char,
    //                     server_uid as libc::c_int,
    //                     folder,
    //                 );
    //             } else {
    //                 /* server response is fine, however, there is no such message, do not try to fetch the message again */
    //                 msg_att = (if !cur.is_null() {
    //                     (*cur).data
    //                 } else {
    //                     0 as *mut libc::c_void
    //                 }) as *mut mailimap_msg_att;
    //                 peek_body(
    //                     msg_att,
    //                     &mut msg_content,
    //                     &mut msg_bytes,
    //                     &mut flags,
    //                     &mut deleted,
    //                 );
    //                 if !(msg_content.is_null() || msg_bytes <= 0 || 0 != deleted) {
    //                     /* dc_log_warning(imap->context, 0, "Message #%i in folder \"%s\" is empty or deleted.", (int)server_uid, folder); -- this is a quite usual situation, do not print a warning */
    //                     imap.receive_imf.expect("non-null function pointer")(
    //                         context,
    //                         msg_content,
    //                         msg_bytes,
    //                         folder,
    //                         server_uid,
    //                         flags,
    //                     );
    //                 }
    //             }
    //         }
    //     }

    //     if !fetch_result.is_null() {
    //         mailimap_fetch_list_free(fetch_result);
    //         fetch_result = 0 as *mut clist
    //     }
    //     if 0 != retry_later {
    //         0
    //     } else {
    //         1
    //     }
    // }

    // unsafe fn peek_body(
    //     msg_att: *mut mailimap_msg_att,
    //     p_msg: *mut *mut libc::c_char,
    //     p_msg_bytes: *mut size_t,
    //     flags: *mut uint32_t,
    //     deleted: *mut libc::c_int,
    // ) {
    //     if msg_att.is_null() {
    //         return;
    //     }
    //     /* search body & Co. in a list of attributes returned by a FETCH command */
    //     let mut iter1: *mut clistiter = 0 as *mut clistiter;
    //     let mut iter2: *mut clistiter = 0 as *mut clistiter;
    //     iter1 = (*(*msg_att).att_list).first;
    //     while !iter1.is_null() {
    //         let mut item: *mut mailimap_msg_att_item = (if !iter1.is_null() {
    //             (*iter1).data
    //         } else {
    //             0 as *mut libc::c_void
    //         }) as *mut mailimap_msg_att_item;
    //         if !item.is_null() {
    //             if (*item).att_type == MAILIMAP_MSG_ATT_ITEM_DYNAMIC as libc::c_int {
    //                 if !(*(*item).att_data.att_dyn).att_list.is_null() {
    //                     iter2 = (*(*(*item).att_data.att_dyn).att_list).first;
    //                     while !iter2.is_null() {
    //                         let mut flag_fetch: *mut mailimap_flag_fetch = (if !iter2.is_null() {
    //                             (*iter2).data
    //                         } else {
    //                             0 as *mut libc::c_void
    //                         })
    //                             as *mut mailimap_flag_fetch;
    //                         if !flag_fetch.is_null()
    //                             && (*flag_fetch).fl_type == MAILIMAP_FLAG_FETCH_OTHER as libc::c_int
    //                         {
    //                             let mut flag: *mut mailimap_flag = (*flag_fetch).fl_flag;
    //                             if !flag.is_null() {
    //                                 if (*flag).fl_type == MAILIMAP_FLAG_SEEN as libc::c_int {
    //                                     *flags = (*flags as libc::c_long | 0x1) as uint32_t
    //                                 } else if (*flag).fl_type
    //                                     == MAILIMAP_FLAG_DELETED as libc::c_int
    //                                 {
    //                                     *deleted = 1
    //                                 }
    //                             }
    //                         }
    //                         iter2 = if !iter2.is_null() {
    //                             (*iter2).next
    //                         } else {
    //                             0 as *mut clistcell_s
    //                         }
    //                     }
    //                 }
    //             } else if (*item).att_type == MAILIMAP_MSG_ATT_ITEM_STATIC as libc::c_int {
    //                 if (*(*item).att_data.att_static).att_type
    //                     == MAILIMAP_MSG_ATT_BODY_SECTION as libc::c_int
    //                 {
    //                     *p_msg = (*(*(*item).att_data.att_static).att_data.att_body_section)
    //                         .sec_body_part;
    //                     *p_msg_bytes =
    //                         (*(*(*item).att_data.att_static).att_data.att_body_section).sec_length;
    //                 }
    //             }
    //         }
    //         iter1 = if !iter1.is_null() {
    //             (*iter1).next
    //         } else {
    //             0 as *mut clistcell_s
    //         };
    //     }
    // }

    // unsafe fn get_config_lastseenuid(
    //     context: &dc_context_t,
    //     imap: &dc_imap_t,
    //     folder: *const libc::c_char,
    //     uidvalidity: *mut uint32_t,
    //     lastseenuid: *mut uint32_t,
    // ) {
    //     *uidvalidity = 0 as uint32_t;
    //     *lastseenuid = 0 as uint32_t;
    //     let mut key: *mut libc::c_char = dc_mprintf(
    //         b"imap.mailbox.%s\x00" as *const u8 as *const libc::c_char,
    //         folder,
    //     );
    //     let mut val1: *mut libc::c_char = imap.get_config.expect("non-null function pointer")(
    //         context,
    //         key,
    //         0 as *const libc::c_char,
    //     );
    //     let mut val2: *mut libc::c_char = 0 as *mut libc::c_char;
    //     let mut val3: *mut libc::c_char = 0 as *mut libc::c_char;
    //     if !val1.is_null() {
    //         val2 = strchr(val1, ':' as i32);
    //         if !val2.is_null() {
    //             *val2 = 0 as libc::c_char;
    //             val2 = val2.offset(1isize);
    //             val3 = strchr(val2, ':' as i32);
    //             if !val3.is_null() {
    //                 *val3 = 0 as libc::c_char
    //             }
    //             *uidvalidity = atol(val1) as uint32_t;
    //             *lastseenuid = atol(val2) as uint32_t
    //         }
    //     }
    //     free(val1 as *mut libc::c_void);
    //     free(key as *mut libc::c_void);
    // }

    // /* ******************************************************************************
    //  * Handle folders
    //  ******************************************************************************/
    // unsafe fn select_folder(
    //     context: &dc_context_t,
    //     imap: &dc_imap_t,
    //     folder: *const libc::c_char,
    // ) -> libc::c_int {
    //     if imap.etpan.is_null() {
    //         *imap.selected_folder.offset(0isize) = 0 as libc::c_char;
    //         imap.selected_folder_needs_expunge = 0;
    //         return 0;
    //     }
    //     if !folder.is_null()
    //         && 0 != *folder.offset(0isize) as libc::c_int
    //         && strcmp(imap.selected_folder, folder) == 0
    //     {
    //         return 1;
    //     }
    //     if 0 != imap.selected_folder_needs_expunge {
    //         if 0 != *imap.selected_folder.offset(0isize) {
    //             dc_log_info(
    //                 context,
    //                 0,
    //                 b"Expunge messages in \"%s\".\x00" as *const u8 as *const libc::c_char,
    //                 imap.selected_folder,
    //             );
    //             mailimap_close(imap.etpan);
    //         }
    //         imap.selected_folder_needs_expunge = 0
    //     }
    //     if !folder.is_null() {
    //         let mut r: libc::c_int = mailimap_select(imap.etpan, folder);
    //         if 0 != dc_imap_is_error(context, imap, r)
    //             || (*imap.etpan).imap_selection_info.is_null()
    //         {
    //             dc_log_info(
    //                 context,
    //                 0,
    //                 b"Cannot select folder; code=%i, imap_response=%s\x00" as *const u8
    //                     as *const libc::c_char,
    //                 r,
    //                 if !(*imap.etpan).imap_response.is_null() {
    //                     (*imap.etpan).imap_response
    //                 } else {
    //                     b"<none>\x00" as *const u8 as *const libc::c_char
    //                 },
    //             );
    //             *imap.selected_folder.offset(0isize) = 0 as libc::c_char;
    //             return 0;
    //         }
    //     }
    //     free(imap.selected_folder as *mut libc::c_void);
    //     imap.selected_folder = dc_strdup(folder);
    //     1
    // }

    pub fn idle(&self, context: &dc_context_t) {
        // unimplemented!()
        println!("starting to idle");
    }
    //     let mut current_block: u64;
    //     let mut r: libc::c_int = 0;
    //     let mut r2: libc::c_int = 0;
    //     if 0 != imap.can_idle {
    //         setup_handle_if_needed(context, imap);
    //         if imap.idle_set_up == 0
    //             && !imap.etpan.is_null()
    //             && !(*imap.etpan).imap_stream.is_null()
    //         {
    //             r = mailstream_setup_idle((*imap.etpan).imap_stream);
    //             if 0 != dc_imap_is_error(context, imap, r) {
    //                 dc_log_warning(
    //                     context,
    //                     0,
    //                     b"IMAP-IDLE: Cannot setup.\x00" as *const u8 as *const libc::c_char,
    //                 );
    //                 fake_idle(context, imap);
    //                 current_block = 14832935472441733737;
    //             } else {
    //                 imap.idle_set_up = 1;
    //                 current_block = 17965632435239708295;
    //             }
    //         } else {
    //             current_block = 17965632435239708295;
    //         }
    //         match current_block {
    //             14832935472441733737 => {}
    //             _ => {
    //                 if 0 == imap.idle_set_up || 0 == select_folder(context, imap, imap.watch_folder)
    //                 {
    //                     dc_log_warning(
    //                         context,
    //                         0,
    //                         b"IMAP-IDLE not setup.\x00" as *const u8 as *const libc::c_char,
    //                     );
    //                     fake_idle(context, imap);
    //                 } else {
    //                     r = mailimap_idle(imap.etpan);
    //                     if 0 != dc_imap_is_error(context, imap, r) {
    //                         dc_log_warning(
    //                             context,
    //                             0,
    //                             b"IMAP-IDLE: Cannot start.\x00" as *const u8 as *const libc::c_char,
    //                         );
    //                         fake_idle(context, imap);
    //                     } else {
    //                         r = mailstream_wait_idle((*imap.etpan).imap_stream, 23 * 60);
    //                         r2 = mailimap_idle_done(imap.etpan);
    //                         if r == MAILSTREAM_IDLE_ERROR as libc::c_int
    //                             || r == MAILSTREAM_IDLE_CANCELLED as libc::c_int
    //                         {
    //                             dc_log_info(
    //                             context,
    //                             0,
    //                             b"IMAP-IDLE wait cancelled, r=%i, r2=%i; we\'ll reconnect soon.\x00"
    //                                 as *const u8
    //                                 as *const libc::c_char,
    //                             r,
    //                             r2,
    //                         );
    //                             imap.should_reconnect = 1
    //                         } else if r == MAILSTREAM_IDLE_INTERRUPTED as libc::c_int {
    //                             dc_log_info(
    //                                 context,
    //                                 0,
    //                                 b"IMAP-IDLE interrupted.\x00" as *const u8
    //                                     as *const libc::c_char,
    //                             );
    //                         } else if r == MAILSTREAM_IDLE_HASDATA as libc::c_int {
    //                             dc_log_info(
    //                                 context,
    //                                 0,
    //                                 b"IMAP-IDLE has data.\x00" as *const u8 as *const libc::c_char,
    //                             );
    //                         } else if r == MAILSTREAM_IDLE_TIMEOUT as libc::c_int {
    //                             dc_log_info(
    //                                 context,
    //                                 0,
    //                                 b"IMAP-IDLE timeout.\x00" as *const u8 as *const libc::c_char,
    //                             );
    //                         } else {
    //                             dc_log_warning(
    //                                 context,
    //                                 0,
    //                                 b"IMAP-IDLE returns unknown value r=%i, r2=%i.\x00" as *const u8
    //                                     as *const libc::c_char,
    //                                 r,
    //                                 r2,
    //                             );
    //                         }
    //                     }
    //                 }
    //             }
    //         }
    //     } else {
    //         fake_idle(context, imap);
    //     }
    // }

    // unsafe fn fake_idle(context: &dc_context_t, imap: &dc_imap_t) {
    //     /* Idle using timeouts. This is also needed if we're not yet configured -
    //     in this case, we're waiting for a configure job */
    //     let mut fake_idle_start_time = SystemTime::now();

    //     dc_log_info(
    //         context,
    //         0,
    //         b"IMAP-fake-IDLEing...\x00" as *const u8 as *const libc::c_char,
    //     );
    //     let mut do_fake_idle: libc::c_int = 1;
    //     while 0 != do_fake_idle {
    //         let seconds_to_wait =
    //             if fake_idle_start_time.elapsed().unwrap() < Duration::new(3 * 60, 0) {
    //                 Duration::new(5, 0)
    //             } else {
    //                 Duration::new(60, 0)
    //             };

    //         let &(ref lock, ref cvar) = &*imap.watch.clone();

    //         let mut watch = lock.lock().unwrap();

    //         loop {
    //             let res = cvar.wait_timeout(watch, seconds_to_wait).unwrap();
    //             watch = res.0;
    //             if *watch {
    //                 do_fake_idle = 0;
    //             }
    //             if *watch || res.1.timed_out() {
    //                 break;
    //             }
    //         }

    //         *watch = false;
    //         if do_fake_idle == 0 {
    //             return;
    //         }
    //         if 0 != setup_handle_if_needed(context, imap) {
    //             if 0 != fetch_from_single_folder(context, imap, imap.watch_folder) {
    //                 do_fake_idle = 0;
    //             }
    //         }
    //     }
    // }

    pub fn interrupt_idle(&self) {
        // unimplemented!();
        println!("interrupt idle");
    }

    //     println!("imap interrupt");
    //     if 0 != imap.can_idle {
    //         if !imap.etpan.is_null() && !(*imap.etpan).imap_stream.is_null() {
    //             mailstream_interrupt_idle((*imap.etpan).imap_stream);
    //         }
    //     }

    //     println!("waiting for lock");
    //     let &(ref lock, ref cvar) = &*imap.watch.clone();
    //     let mut watch = lock.lock().unwrap();

    //     *watch = true;
    //     println!("notify");
    //     cvar.notify_one();
    // }

    pub fn mv(
        &self,
        context: &dc_context_t,
        folder: *const libc::c_char,
        uid: uint32_t,
        dest_folder: *const libc::c_char,
        dest_uid: *mut uint32_t,
    ) -> dc_imap_res {
        unimplemented!()
    }
    //     let mut current_block: u64;
    //     let mut res: dc_imap_res = DC_RETRY_LATER;
    //     let mut r: libc::c_int = 0;
    //     let mut set: *mut mailimap_set = mailimap_set_new_single(uid);
    //     let mut res_uid: uint32_t = 0 as uint32_t;
    //     let mut res_setsrc: *mut mailimap_set = 0 as *mut mailimap_set;
    //     let mut res_setdest: *mut mailimap_set = 0 as *mut mailimap_set;
    //     if folder.is_null()
    //         || uid == 0 as libc::c_uint
    //         || dest_folder.is_null()
    //         || dest_uid.is_null()
    //         || set.is_null()
    //     {
    //         res = DC_FAILED
    //     } else if strcasecmp(folder, dest_folder) == 0 {
    //         dc_log_info(
    //             context,
    //             0,
    //             b"Skip moving message; message %s/%i is already in %s...\x00" as *const u8
    //                 as *const libc::c_char,
    //             folder,
    //             uid as libc::c_int,
    //             dest_folder,
    //         );
    //         res = DC_ALREADY_DONE
    //     } else {
    //         dc_log_info(
    //             context,
    //             0,
    //             b"Moving message %s/%i to %s...\x00" as *const u8 as *const libc::c_char,
    //             folder,
    //             uid as libc::c_int,
    //             dest_folder,
    //         );
    //         if select_folder(context, imap, folder) == 0 {
    //             dc_log_warning(
    //                 context,
    //                 0,
    //                 b"Cannot select folder %s for moving message.\x00" as *const u8
    //                     as *const libc::c_char,
    //                 folder,
    //             );
    //         } else {
    //             r = mailimap_uidplus_uid_move(
    //                 imap.etpan,
    //                 set,
    //                 dest_folder,
    //                 &mut res_uid,
    //                 &mut res_setsrc,
    //                 &mut res_setdest,
    //             );
    //             if 0 != dc_imap_is_error(context, imap, r) {
    //                 if !res_setsrc.is_null() {
    //                     mailimap_set_free(res_setsrc);
    //                     res_setsrc = 0 as *mut mailimap_set
    //                 }
    //                 if !res_setdest.is_null() {
    //                     mailimap_set_free(res_setdest);
    //                     res_setdest = 0 as *mut mailimap_set
    //                 }
    //                 dc_log_info(
    //                     context,
    //                     0,
    //                     b"Cannot move message, fallback to COPY/DELETE %s/%i to %s...\x00"
    //                         as *const u8 as *const libc::c_char,
    //                     folder,
    //                     uid as libc::c_int,
    //                     dest_folder,
    //                 );
    //                 r = mailimap_uidplus_uid_copy(
    //                     imap.etpan,
    //                     set,
    //                     dest_folder,
    //                     &mut res_uid,
    //                     &mut res_setsrc,
    //                     &mut res_setdest,
    //                 );
    //                 if 0 != dc_imap_is_error(context, imap, r) {
    //                     dc_log_info(
    //                         context,
    //                         0,
    //                         b"Cannot copy message.\x00" as *const u8 as *const libc::c_char,
    //                     );
    //                     current_block = 14415637129417834392;
    //                 } else {
    //                     if add_flag(imap, uid, mailimap_flag_new_deleted()) == 0 {
    //                         dc_log_warning(
    //                             context,
    //                             0,
    //                             b"Cannot mark message as \"Deleted\".\x00" as *const u8
    //                                 as *const libc::c_char,
    //                         );
    //                     }
    //                     imap.selected_folder_needs_expunge = 1;
    //                     current_block = 1538046216550696469;
    //                 }
    //             } else {
    //                 current_block = 1538046216550696469;
    //             }
    //             match current_block {
    //                 14415637129417834392 => {}
    //                 _ => {
    //                     if !res_setdest.is_null() {
    //                         let mut cur: *mut clistiter = (*(*res_setdest).set_list).first;
    //                         if !cur.is_null() {
    //                             let mut item: *mut mailimap_set_item = 0 as *mut mailimap_set_item;
    //                             item = (if !cur.is_null() {
    //                                 (*cur).data
    //                             } else {
    //                                 0 as *mut libc::c_void
    //                             }) as *mut mailimap_set_item;
    //                             *dest_uid = (*item).set_first
    //                         }
    //                     }
    //                     res = DC_SUCCESS
    //                 }
    //             }
    //         }
    //     }
    //     if !set.is_null() {
    //         mailimap_set_free(set);
    //         set = 0 as *mut mailimap_set
    //     }
    //     if !res_setsrc.is_null() {
    //         mailimap_set_free(res_setsrc);
    //         res_setsrc = 0 as *mut mailimap_set
    //     }
    //     if !res_setdest.is_null() {
    //         mailimap_set_free(res_setdest);
    //         res_setdest = 0 as *mut mailimap_set
    //     }
    //     return (if res as libc::c_uint == DC_RETRY_LATER as libc::c_int as libc::c_uint {
    //         (if 0 != imap.should_reconnect {
    //             DC_RETRY_LATER as libc::c_int
    //         } else {
    //             DC_FAILED as libc::c_int
    //         }) as libc::c_uint
    //     } else {
    //         res as libc::c_uint
    //     }) as dc_imap_res;
    // }

    // unsafe fn add_flag(
    //     imap: &dc_imap_t,
    //     server_uid: uint32_t,
    //     flag: *mut mailimap_flag,
    // ) -> libc::c_int {
    //     let mut flag_list: *mut mailimap_flag_list = 0 as *mut mailimap_flag_list;
    //     let mut store_att_flags: *mut mailimap_store_att_flags = 0 as *mut mailimap_store_att_flags;
    //     let mut set: *mut mailimap_set = mailimap_set_new_single(server_uid);
    //     if !(imap.etpan.is_null()) {
    //         flag_list = mailimap_flag_list_new_empty();
    //         mailimap_flag_list_add(flag_list, flag);
    //         store_att_flags = mailimap_store_att_flags_new_add_flags(flag_list);
    //         mailimap_uid_store(imap.etpan, set, store_att_flags);
    //     }
    //     if !store_att_flags.is_null() {
    //         mailimap_store_att_flags_free(store_att_flags);
    //     }
    //     if !set.is_null() {
    //         mailimap_set_free(set);
    //         set = 0 as *mut mailimap_set
    //     }
    //     if 0 != imap.should_reconnect {
    //         0
    //     } else {
    //         1
    //     }
    // }

    pub fn set_seen(
        &self,
        context: &dc_context_t,
        folder: *const libc::c_char,
        uid: uint32_t,
    ) -> dc_imap_res {
        unimplemented!()
    }
    //     let mut res: dc_imap_res = DC_RETRY_LATER;
    //     if folder.is_null() || uid == 0 as libc::c_uint {
    //         res = DC_FAILED
    //     } else if !imap.etpan.is_null() {
    //         dc_log_info(
    //             context,
    //             0,
    //             b"Marking message %s/%i as seen...\x00" as *const u8 as *const libc::c_char,
    //             folder,
    //             uid as libc::c_int,
    //         );
    //         if select_folder(context, imap, folder) == 0 {
    //             dc_log_warning(
    //                 context,
    //                 0,
    //                 b"Cannot select folder %s for setting SEEN flag.\x00" as *const u8
    //                     as *const libc::c_char,
    //                 folder,
    //             );
    //         } else if add_flag(imap, uid, mailimap_flag_new_seen()) == 0 {
    //             dc_log_warning(
    //                 context,
    //                 0,
    //                 b"Cannot mark message as seen.\x00" as *const u8 as *const libc::c_char,
    //             );
    //         } else {
    //             res = DC_SUCCESS
    //         }
    //     }
    //     return (if res as libc::c_uint == DC_RETRY_LATER as libc::c_int as libc::c_uint {
    //         (if 0 != imap.should_reconnect {
    //             DC_RETRY_LATER as libc::c_int
    //         } else {
    //             DC_FAILED as libc::c_int
    //         }) as libc::c_uint
    //     } else {
    //         res as libc::c_uint
    //     }) as dc_imap_res;
    // }

    pub fn set_mdnsent(
        &self,
        context: &dc_context_t,
        folder: *const libc::c_char,
        uid: uint32_t,
    ) -> dc_imap_res {
        unimplemented!();
    }

    //     let mut can_create_flag: libc::c_int = 0;
    //     let mut current_block: u64;
    //     // returns 0=job should be retried later, 1=job done, 2=job done and flag just set
    //     let mut res: dc_imap_res = DC_RETRY_LATER;
    //     let mut set: *mut mailimap_set = mailimap_set_new_single(uid);
    //     let mut fetch_result: *mut clist = 0 as *mut clist;
    //     if folder.is_null() || uid == 0 as libc::c_uint || set.is_null() {
    //         res = DC_FAILED
    //     } else if !imap.etpan.is_null() {
    //         dc_log_info(
    //             context,
    //             0,
    //             b"Marking message %s/%i as $MDNSent...\x00" as *const u8 as *const libc::c_char,
    //             folder,
    //             uid as libc::c_int,
    //         );
    //         if select_folder(context, imap, folder) == 0 {
    //             dc_log_warning(
    //                 context,
    //                 0,
    //                 b"Cannot select folder %s for setting $MDNSent flag.\x00" as *const u8
    //                     as *const libc::c_char,
    //                 folder,
    //             );
    //         } else {
    //             /* Check if the folder can handle the `$MDNSent` flag (see RFC 3503).  If so, and not set: set the flags and return this information.
    //             If the folder cannot handle the `$MDNSent` flag, we risk duplicated MDNs; it's up to the receiving MUA to handle this then (eg. Delta Chat has no problem with this). */
    //             can_create_flag = 0;
    //             if !(*imap.etpan).imap_selection_info.is_null()
    //                 && !(*(*imap.etpan).imap_selection_info)
    //                     .sel_perm_flags
    //                     .is_null()
    //             {
    //                 let mut iter: *mut clistiter = 0 as *mut clistiter;
    //                 iter = (*(*(*imap.etpan).imap_selection_info).sel_perm_flags).first;
    //                 while !iter.is_null() {
    //                     let mut fp: *mut mailimap_flag_perm = (if !iter.is_null() {
    //                         (*iter).data
    //                     } else {
    //                         0 as *mut libc::c_void
    //                     })
    //                         as *mut mailimap_flag_perm;
    //                     if !fp.is_null() {
    //                         if (*fp).fl_type == MAILIMAP_FLAG_PERM_ALL as libc::c_int {
    //                             can_create_flag = 1;
    //                             break;
    //                         } else if (*fp).fl_type == MAILIMAP_FLAG_PERM_FLAG as libc::c_int
    //                             && !(*fp).fl_flag.is_null()
    //                         {
    //                             let mut fl: *mut mailimap_flag =
    //                                 (*fp).fl_flag as *mut mailimap_flag;
    //                             if (*fl).fl_type == MAILIMAP_FLAG_KEYWORD as libc::c_int
    //                                 && !(*fl).fl_data.fl_keyword.is_null()
    //                                 && strcmp(
    //                                     (*fl).fl_data.fl_keyword,
    //                                     b"$MDNSent\x00" as *const u8 as *const libc::c_char,
    //                                 ) == 0
    //                             {
    //                                 can_create_flag = 1;
    //                                 break;
    //                             }
    //                         }
    //                     }
    //                     iter = if !iter.is_null() {
    //                         (*iter).next
    //                     } else {
    //                         0 as *mut clistcell_s
    //                     }
    //                 }
    //             }
    //             if 0 != can_create_flag {
    //                 let mut r: libc::c_int = mailimap_uid_fetch(
    //                     imap.etpan,
    //                     set,
    //                     imap.fetch_type_flags,
    //                     &mut fetch_result,
    //                 );
    //                 if 0 != dc_imap_is_error(context, imap, r) || fetch_result.is_null() {
    //                     fetch_result = 0 as *mut clist
    //                 } else {
    //                     let mut cur: *mut clistiter = (*fetch_result).first;
    //                     if !cur.is_null() {
    //                         if 0 != peek_flag_keyword(
    //                             (if !cur.is_null() {
    //                                 (*cur).data
    //                             } else {
    //                                 0 as *mut libc::c_void
    //                             }) as *mut mailimap_msg_att,
    //                             b"$MDNSent\x00" as *const u8 as *const libc::c_char,
    //                         ) {
    //                             res = DC_ALREADY_DONE;
    //                             current_block = 14832935472441733737;
    //                         } else if add_flag(
    //                             imap,
    //                             uid,
    //                             mailimap_flag_new_flag_keyword(dc_strdup(
    //                                 b"$MDNSent\x00" as *const u8 as *const libc::c_char,
    //                             )),
    //                         ) == 0
    //                         {
    //                             current_block = 17044610252497760460;
    //                         } else {
    //                             res = DC_SUCCESS;
    //                             current_block = 14832935472441733737;
    //                         }
    //                         match current_block {
    //                             17044610252497760460 => {}
    //                             _ => {
    //                                 dc_log_info(
    //                                     context,
    //                                     0,
    //                                     if res as libc::c_uint
    //                                         == DC_SUCCESS as libc::c_int as libc::c_uint
    //                                     {
    //                                         b"$MDNSent just set and MDN will be sent.\x00"
    //                                             as *const u8
    //                                             as *const libc::c_char
    //                                     } else {
    //                                         b"$MDNSent already set and MDN already sent.\x00"
    //                                             as *const u8
    //                                             as *const libc::c_char
    //                                     },
    //                                 );
    //                             }
    //                         }
    //                     }
    //                 }
    //             } else {
    //                 res = DC_SUCCESS;
    //                 dc_log_info(
    //                     context,
    //                     0,
    //                     b"Cannot store $MDNSent flags, risk sending duplicate MDN.\x00" as *const u8
    //                         as *const libc::c_char,
    //                 );
    //             }
    //         }
    //     }
    //     if !set.is_null() {
    //         mailimap_set_free(set);
    //         set = 0 as *mut mailimap_set
    //     }
    //     if !fetch_result.is_null() {
    //         mailimap_fetch_list_free(fetch_result);
    //         fetch_result = 0 as *mut clist
    //     }

    //     (if res as libc::c_uint == DC_RETRY_LATER as libc::c_int as libc::c_uint {
    //         (if 0 != imap.should_reconnect {
    //             DC_RETRY_LATER as libc::c_int
    //         } else {
    //             DC_FAILED as libc::c_int
    //         }) as libc::c_uint
    //     } else {
    //         res as libc::c_uint
    //     }) as dc_imap_res
    // }

    // unsafe fn peek_flag_keyword(
    //     msg_att: *mut mailimap_msg_att,
    //     flag_keyword: *const libc::c_char,
    // ) -> libc::c_int {
    //     if msg_att.is_null() || (*msg_att).att_list.is_null() || flag_keyword.is_null() {
    //         return 0;
    //     }
    //     let mut iter1: *mut clistiter = 0 as *mut clistiter;
    //     let mut iter2: *mut clistiter = 0 as *mut clistiter;
    //     iter1 = (*(*msg_att).att_list).first;
    //     while !iter1.is_null() {
    //         let mut item: *mut mailimap_msg_att_item = (if !iter1.is_null() {
    //             (*iter1).data
    //         } else {
    //             0 as *mut libc::c_void
    //         }) as *mut mailimap_msg_att_item;
    //         if !item.is_null() {
    //             if (*item).att_type == MAILIMAP_MSG_ATT_ITEM_DYNAMIC as libc::c_int {
    //                 if !(*(*item).att_data.att_dyn).att_list.is_null() {
    //                     iter2 = (*(*(*item).att_data.att_dyn).att_list).first;
    //                     while !iter2.is_null() {
    //                         let mut flag_fetch: *mut mailimap_flag_fetch = (if !iter2.is_null() {
    //                             (*iter2).data
    //                         } else {
    //                             0 as *mut libc::c_void
    //                         })
    //                             as *mut mailimap_flag_fetch;
    //                         if !flag_fetch.is_null()
    //                             && (*flag_fetch).fl_type == MAILIMAP_FLAG_FETCH_OTHER as libc::c_int
    //                         {
    //                             let mut flag: *mut mailimap_flag = (*flag_fetch).fl_flag;
    //                             if !flag.is_null() {
    //                                 if (*flag).fl_type == MAILIMAP_FLAG_KEYWORD as libc::c_int
    //                                     && !(*flag).fl_data.fl_keyword.is_null()
    //                                     && strcmp((*flag).fl_data.fl_keyword, flag_keyword) == 0
    //                                 {
    //                                     return 1;
    //                                 }
    //                             }
    //                         }
    //                         iter2 = if !iter2.is_null() {
    //                             (*iter2).next
    //                         } else {
    //                             0 as *mut clistcell_s
    //                         }
    //                     }
    //                 }
    //             }
    //         }
    //         iter1 = if !iter1.is_null() {
    //             (*iter1).next
    //         } else {
    //             0 as *mut clistcell_s
    //         }
    //     }
    //     0
    // }

    // only returns 0 on connection problems; we should try later again in this case *
    pub unsafe fn delete_msg(
        &self,
        context: &dc_context_t,
        rfc724_mid: *const libc::c_char,
        folder: *const libc::c_char,
        mut server_uid: uint32_t,
    ) -> libc::c_int {
        unimplemented!()
    }
    //     let mut success: libc::c_int = 0;
    //     let mut r: libc::c_int = 0;
    //     let mut fetch_result: *mut clist = 0 as *mut clist;
    //     let mut is_rfc724_mid: *mut libc::c_char = 0 as *mut libc::c_char;
    //     let mut new_folder: *mut libc::c_char = 0 as *mut libc::c_char;
    //     if rfc724_mid.is_null()
    //         || folder.is_null()
    //         || *folder.offset(0isize) as libc::c_int == 0
    //         || server_uid == 0 as libc::c_uint
    //     {
    //         success = 1
    //     } else {
    //         dc_log_info(
    //             context,
    //             0,
    //             b"Marking message \"%s\", %s/%i for deletion...\x00" as *const u8
    //                 as *const libc::c_char,
    //             rfc724_mid,
    //             folder,
    //             server_uid as libc::c_int,
    //         );
    //         if select_folder(context, imap, folder) == 0 {
    //             dc_log_warning(
    //                 context,
    //                 0,
    //                 b"Cannot select folder %s for deleting message.\x00" as *const u8
    //                     as *const libc::c_char,
    //                 folder,
    //             );
    //         } else {
    //             let mut cur: *mut clistiter = 0 as *mut clistiter;
    //             let mut is_quoted_rfc724_mid: *const libc::c_char = 0 as *const libc::c_char;
    //             let mut set: *mut mailimap_set = mailimap_set_new_single(server_uid);
    //             r = mailimap_uid_fetch(
    //                 imap.etpan,
    //                 set,
    //                 imap.fetch_type_prefetch,
    //                 &mut fetch_result,
    //             );
    //             if !set.is_null() {
    //                 mailimap_set_free(set);
    //                 set = 0 as *mut mailimap_set
    //             }
    //             if 0 != dc_imap_is_error(context, imap, r) || fetch_result.is_null() {
    //                 fetch_result = 0 as *mut clist;
    //                 dc_log_warning(
    //                     context,
    //                     0,
    //                     b"Cannot delete on IMAP, %s/%i not found.\x00" as *const u8
    //                         as *const libc::c_char,
    //                     folder,
    //                     server_uid as libc::c_int,
    //                 );
    //                 server_uid = 0 as uint32_t
    //             }
    //             cur = (*fetch_result).first;
    //             if cur.is_null()
    //                 || {
    //                     is_quoted_rfc724_mid = peek_rfc724_mid(
    //                         (if !cur.is_null() {
    //                             (*cur).data
    //                         } else {
    //                             0 as *mut libc::c_void
    //                         }) as *mut mailimap_msg_att,
    //                     );
    //                     is_quoted_rfc724_mid.is_null()
    //                 }
    //                 || {
    //                     is_rfc724_mid = unquote_rfc724_mid(is_quoted_rfc724_mid);
    //                     is_rfc724_mid.is_null()
    //                 }
    //                 || strcmp(is_rfc724_mid, rfc724_mid) != 0
    //             {
    //                 dc_log_warning(
    //                     context,
    //                     0,
    //                     b"Cannot delete on IMAP, %s/%i does not match %s.\x00" as *const u8
    //                         as *const libc::c_char,
    //                     folder,
    //                     server_uid as libc::c_int,
    //                     rfc724_mid,
    //                 );
    //                 server_uid = 0 as uint32_t
    //             }
    //             /* mark the message for deletion */
    //             if add_flag(imap, server_uid, mailimap_flag_new_deleted()) == 0 {
    //                 dc_log_warning(
    //                     context,
    //                     0,
    //                     b"Cannot mark message as \"Deleted\".\x00" as *const u8
    //                         as *const libc::c_char,
    //                 );
    //             } else {
    //                 imap.selected_folder_needs_expunge = 1;
    //                 success = 1
    //             }
    //         }
    //     }
    //     if !fetch_result.is_null() {
    //         mailimap_fetch_list_free(fetch_result);
    //         fetch_result = 0 as *mut clist
    //     }
    //     free(is_rfc724_mid as *mut libc::c_void);
    //     free(new_folder as *mut libc::c_void);

    //     if 0 != success {
    //         1
    //     } else {
    //         dc_imap_is_connected(imap)
    //     }
    // }

    pub fn configure_folders(&self, context: &dc_context_t, flags: libc::c_int) {
        if !self.is_connected() {
            return;
        }

        unsafe {
            dc_log_info(
                context,
                0,
                b"Configuring IMAP-folders.\x00" as *const u8 as *const libc::c_char,
            )
        };

        let folders = self.list_folders(context).unwrap();
        let delimiter = self.config.read().unwrap().imap_delimiter;
        let fallback_folder = format!("INBOX{}DeltaChat", delimiter);

        for folder in folders.iter() {
            let meaning = get_folder_meaning(folder);
            println!("{} - {:?}", folder.name(), meaning);
        }
        // let iter = (*folder_list).first;
        // while !iter.is_null() {
        //     let mut folder: *mut dc_imapfolder_t = (if !iter.is_null() {
        //         (*iter).data
        //     } else {
        //         0 as *mut libc::c_void
        //     }) as *mut dc_imapfolder_t;
        //     if strcmp(
        //         (*folder).name_utf8,
        //         b"DeltaChat\x00" as *const u8 as *const libc::c_char,
        //     ) == 0i32
        //         || strcmp((*folder).name_utf8, fallback_folder) == 0i32
        //     {
        //         if mvbox_folder.is_null() {
        //             mvbox_folder = dc_strdup((*folder).name_to_select)
        //         }
        //     }
        //     if (*folder).meaning == 1i32 {
        //         if sentbox_folder.is_null() {
        //             sentbox_folder = dc_strdup((*folder).name_to_select)
        //         }
        //     }
        //     iter = if !iter.is_null() {
        //         (*iter).next
        //     } else {
        //         0 as *mut clistcell_s
        //     }
        // }
        // if mvbox_folder.is_null() && 0 != flags & 0x1i32 {
        //     dc_log_info(
        //         context,
        //         0i32,
        //         b"Creating MVBOX-folder \"%s\"...\x00" as *const u8 as *const libc::c_char,
        //         b"DeltaChat\x00" as *const u8 as *const libc::c_char,
        //     );
        //     let mut r: libc::c_int = mailimap_create(
        //         (*imap).etpan,
        //         b"DeltaChat\x00" as *const u8 as *const libc::c_char,
        //     );
        //     if 0 != dc_imap_is_error(context, imap, r) {
        //         dc_log_warning(
        //             context,
        //             0i32,
        //             b"Cannot create MVBOX-folder, using trying INBOX subfolder.\x00" as *const u8
        //                 as *const libc::c_char,
        //         );
        //         r = mailimap_create((*imap).etpan, fallback_folder);
        //         if 0 != dc_imap_is_error(context, imap, r) {
        //             dc_log_warning(
        //                 context,
        //                 0i32,
        //                 b"Cannot create MVBOX-folder.\x00" as *const u8 as *const libc::c_char,
        //             );
        //         } else {
        //             mvbox_folder = dc_strdup(fallback_folder);
        //             dc_log_info(
        //                 context,
        //                 0i32,
        //                 b"MVBOX-folder created as INBOX subfolder.\x00" as *const u8
        //                     as *const libc::c_char,
        //             );
        //         }
        //     } else {
        //         mvbox_folder = dc_strdup(b"DeltaChat\x00" as *const u8 as *const libc::c_char);
        //         dc_log_info(
        //             context,
        //             0i32,
        //             b"MVBOX-folder created.\x00" as *const u8 as *const libc::c_char,
        //         );
        //     }
        //     mailimap_subscribe((*imap).etpan, mvbox_folder);
        // }
        // dc_sqlite3_set_config_int(
        //     context,
        //     &context.sql.clone().read().unwrap(),
        //     b"folders_configured\x00" as *const u8 as *const libc::c_char,
        //     3i32,
        // );
        // dc_sqlite3_set_config(
        //     context,
        //     &context.sql.clone().read().unwrap(),
        //     b"configured_mvbox_folder\x00" as *const u8 as *const libc::c_char,
        //     mvbox_folder,
        // );
        // dc_sqlite3_set_config(
        //     context,
        //     &context.sql.clone().read().unwrap(),
        //     b"configured_sentbox_folder\x00" as *const u8 as *const libc::c_char,
        //     sentbox_folder,
        // );
    }

    fn list_folders(
        &self,
        context: &dc_context_t,
    ) -> Option<imap::types::ZeroCopy<Vec<imap::types::Name>>> {
        if let Some(ref mut session) = *self.session.lock().unwrap() {
            // TODO: use xlist when available
            match session.list(Some(""), Some("*")) {
                Ok(list) => {
                    if list.is_empty() {
                        unsafe {
                            dc_log_warning(
                                context,
                                0i32,
                                b"Folder list is empty.\x00" as *const u8 as *const libc::c_char,
                            )
                        };
                    }
                    Some(list)
                }
                Err(err) => {
                    eprintln!("list error: {:?}", err);
                    unsafe {
                        dc_log_warning(
                            context,
                            0i32,
                            b"Cannot get folder list.\x00" as *const u8 as *const libc::c_char,
                        )
                    };
                    None
                }
            }
        } else {
            None
        }
    }
}

fn to_string(str: *const libc::c_char) -> String {
    unsafe { CStr::from_ptr(str).to_str().unwrap().to_string() }
}

fn to_str<'a>(str: *const libc::c_char) -> &'a str {
    unsafe { CStr::from_ptr(str).to_str().unwrap() }
}

/// Try to get the folder meaning by the name of the folder only used if the server does not support XLIST.
// TODO: lots languages missing - maybe there is a list somewhere on other MUAs?
// however, if we fail to find out the sent-folder,
// only watching this folder is not working. at least, this is no show stopper.
// CAVE: if possible, take care not to add a name here that is "sent" in one language
// but sth. different in others - a hard job.
fn get_folder_meaning_by_name(folder_name: &imap::types::Name) -> FolderMeaning {
    let sent_names = vec!["sent", "sent objects", "gesendet"];
    let lower = folder_name.name().to_lowercase();

    if sent_names.into_iter().find(|s| *s == lower).is_some() {
        FolderMeaning::SentObjects
    } else {
        FolderMeaning::Unknown
    }
}

fn get_folder_meaning(folder_name: &imap::types::Name) -> FolderMeaning {
    if folder_name.attributes().is_empty() {
        return FolderMeaning::Unknown;
    }

    let mut res = FolderMeaning::Unknown;
    let special_names = vec!["\\Spam", "\\Trash", "\\Drafts", "\\Junk"];

    for attr in folder_name.attributes() {
        println!("attr: {:?} - {}", attr, folder_name.name());
        match attr {
            imap::types::NameAttribute::Custom(ref label) => {
                if special_names.iter().find(|s| *s == label).is_some() {
                    res = FolderMeaning::Other;
                } else if label == "\\Sent" {
                    res = FolderMeaning::SentObjects
                }
            }
            _ => {}
        }
    }

    match res {
        FolderMeaning::Unknown => get_folder_meaning_by_name(folder_name),
        _ => res,
    }
}
