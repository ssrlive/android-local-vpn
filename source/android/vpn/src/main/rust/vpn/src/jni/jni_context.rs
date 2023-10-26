use jni::{
    objects::{JMethodID, JObject, JValue},
    signature::{Primitive, ReturnType},
    JNIEnv,
};

pub struct JniContext<'a> {
    pub(super) jni_env: JNIEnv<'a>,
    pub(super) object: &'a JObject<'a>,
    pub(super) protect_method_id: JMethodID,
}

impl<'a> JniContext<'a> {
    pub fn protect_socket(&mut self, socket: i32) -> bool {
        if socket <= 0 {
            log::error!("invalid socket, socket={:?}", socket);
            return false;
        }
        let return_type = ReturnType::Primitive(Primitive::Boolean);
        let arguments = [JValue::Int(socket).as_jni()];
        let result = unsafe {
            self.jni_env
                .call_method_unchecked(self.object, self.protect_method_id, return_type, &arguments[..])
        };
        match result {
            Ok(value) => {
                log::trace!("protected socket, result={:?}", value);
                value.z().unwrap()
            }
            Err(error_code) => {
                log::error!("failed to protect socket, error={:?}", error_code);
                false
            }
        }
    }
}
