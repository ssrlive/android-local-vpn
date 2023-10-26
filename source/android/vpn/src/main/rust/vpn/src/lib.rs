#[macro_use]
mod jni;

#[macro_use]
mod socket_protector;

pub mod android {

    use crate::{jni::Jni, socket_protector::SocketProtector};
    use android_logger::Config;
    use core::{tun, tun_callbacks};
    use jni::{
        objects::{JClass, JObject},
        JNIEnv,
    };

    /// # Safety
    ///
    /// This function should only be used in jni context.
    #[no_mangle]
    pub unsafe extern "C" fn Java_com_github_jonforshort_androidlocalvpn_vpn_LocalVpnService_onCreateNative(env: JNIEnv, class: JClass, object: JObject) {
        android_logger::init_once(Config::default().with_tag("nativeVpn").with_max_level(log::LevelFilter::Trace));
        log::trace!("onCreateNative");
        set_panic_handler();
        Jni::init(env, class, object);
        SocketProtector::init();
        tun::create();
    }

    /// # Safety
    ///
    /// This function should only be used in jni context.
    #[no_mangle]
    pub unsafe extern "C" fn Java_com_github_jonforshort_androidlocalvpn_vpn_LocalVpnService_onDestroyNative(_: JNIEnv, _: JClass) {
        log::trace!("onDestroyNative");
        tun::destroy();
        SocketProtector::release();
        Jni::release();
        remove_panic_handler();
    }

    /// # Safety
    ///
    /// This function should only be used in jni context.
    #[no_mangle]
    pub unsafe extern "C" fn Java_com_github_jonforshort_androidlocalvpn_vpn_LocalVpnService_onStartVpn(_: JNIEnv, _: JClass, file_descriptor: i32) {
        log::trace!("onStartVpn, pid={}, fd={}", std::process::id(), file_descriptor);
        tun_callbacks::set_socket_created_callback(Some(on_socket_created));
        socket_protector!().start();
        tun::start(file_descriptor);
    }

    /// # Safety
    ///
    /// This function should only be used in jni context.
    #[no_mangle]
    pub unsafe extern "C" fn Java_com_github_jonforshort_androidlocalvpn_vpn_LocalVpnService_onStopVpn(_: JNIEnv, _: JClass) {
        log::trace!("onStopVpn, pid={}", std::process::id());
        tun::stop();
        socket_protector!().stop();
        tun_callbacks::set_socket_created_callback(None);
    }

    fn set_panic_handler() {
        std::panic::set_hook(Box::new(|panic_info| {
            log::error!("*** PANIC [{:?}]", panic_info);
        }));
    }

    fn remove_panic_handler() {
        let _ = std::panic::take_hook();
    }

    fn on_socket_created(socket: i32) {
        socket_protector!().protect_socket(socket);
    }
}
