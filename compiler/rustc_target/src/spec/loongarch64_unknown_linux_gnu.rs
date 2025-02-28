use crate::spec::{CodeModel, Target, TargetOptions};

pub fn target() -> Target {
    Target {
        llvm_target: "loongarch64-unknown-linux-gnu".into(),
        pointer_width: 64,
        data_layout: "e-m:e-p:64:64-i64:64-i128:128-n64-S128".into(),
        arch: "loongarch64".into(),
        options: TargetOptions {
            code_model: Some(CodeModel::Medium),
            cpu: "generic-la64".into(),
            features: "+d".into(),
            llvm_abiname: "lp64d".into(),
            max_atomic_width: Some(64),
            ..super::linux_gnu_base::opts()
        },
    }
}
