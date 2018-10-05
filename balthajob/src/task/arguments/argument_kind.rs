use wasmi::nan_preserving_float::{F32, F64};
pub use wasmi::RuntimeValue;

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
pub enum ArgumentKind {
    //TODO: U{32,64,...} as well (or instead)?
    I32(i32),
    I64(i64),
    F32(u32),
    F64(u64),
}

impl ArgumentKind {
    pub fn to_runtime_value(&self) -> RuntimeValue {
        match self {
            ArgumentKind::I32(n) => RuntimeValue::I32(*n),
            ArgumentKind::I64(n) => RuntimeValue::I64(*n),
            ArgumentKind::F32(n) => RuntimeValue::decode_f32(*n),
            ArgumentKind::F64(n) => RuntimeValue::decode_f64(*n),
        }
    }
}

impl From<i8> for ArgumentKind {
    fn from(val: i8) -> Self {
        ArgumentKind::I32(i32::from(val))
    }
}
impl From<i16> for ArgumentKind {
    fn from(val: i16) -> Self {
        ArgumentKind::I32(i32::from(val))
    }
}
impl From<i32> for ArgumentKind {
    fn from(val: i32) -> Self {
        ArgumentKind::I32(val)
    }
}
impl From<i64> for ArgumentKind {
    fn from(val: i64) -> Self {
        ArgumentKind::I64(val)
    }
}

impl From<u8> for ArgumentKind {
    fn from(val: u8) -> Self {
        ArgumentKind::I32(i32::from(val))
    }
}
impl From<u16> for ArgumentKind {
    fn from(val: u16) -> Self {
        ArgumentKind::I32(i32::from(val))
    }
}
impl From<u32> for ArgumentKind {
    fn from(val: u32) -> Self {
        let bytes = val.to_le_bytes();
        ArgumentKind::I32(i32::from_le_bytes(bytes))
    }
}
impl From<u64> for ArgumentKind {
    fn from(val: u64) -> Self {
        let bytes = val.to_le_bytes();
        ArgumentKind::I64(i64::from_le_bytes(bytes))
    }
}

impl From<f32> for ArgumentKind {
    fn from(val: f32) -> Self {
        ArgumentKind::F32(val.to_bits())
    }
}
impl From<f64> for ArgumentKind {
    fn from(val: f64) -> Self {
        ArgumentKind::F64(val.to_bits())
    }
}
impl From<F32> for ArgumentKind {
    fn from(val: F32) -> Self {
        ArgumentKind::from(val.to_float())
    }
}
impl From<F64> for ArgumentKind {
    fn from(val: F64) -> Self {
        ArgumentKind::from(val.to_float())
    }
}

impl From<RuntimeValue> for ArgumentKind {
    fn from(val: RuntimeValue) -> Self {
        match val {
            RuntimeValue::I32(n) => ArgumentKind::from(n),
            RuntimeValue::I64(n) => ArgumentKind::from(n),
            RuntimeValue::F32(n) => ArgumentKind::from(n),
            RuntimeValue::F64(n) => ArgumentKind::from(n),
        }
    }
}
