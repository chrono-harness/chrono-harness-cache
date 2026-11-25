//! 缓存命令采用真实 argv、派发结果及诊断子进程状态时的分支。
#![allow(unexpected_cfgs)]
#[cfg(not(verus_keep_ghost))]
use vstd::prelude::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {

/// 宿主解码 UTF-8；非法参数保留其原字节及两个坐标。
pub fn argument<'a>(index: usize, text: Option<&'a str>, bytes: &'a [u8])
    -> (out: Result<Option<&'a str>, (usize, Option<usize>, &'a [u8])>)
    ensures
        text.is_some() ==> out == Ok::<_, (usize, Option<usize>, &[u8])>(
            if index == 0 { None } else { text }
        ),
        text.is_none() ==> match out {
            Err((argv, arg, raw)) => argv == index && raw == bytes
                && match arg { None => index == 0, Some(i) => i + 1 == index },
            Ok(_) => false,
        },
{
    match text {
        Some(_) if index == 0 => Ok(None),
        Some(value) => Ok(Some(value)),
        None => Err((index, index.checked_sub(1), bytes)),
    }
}

/// 分发原文本到 stdout 或 stderr；业务错误继续交给可选的诊断会话。
pub fn output<T, S, O, E>(state: S, result: Result<T, T>, stdout: O, stderr: E)
    -> (out: (S, Result<u8, T>))
    where O: FnOnce(S, &T) -> S, E: FnOnce(S, &T) -> S,
    requires
        forall|s: S, t: &T| stdout.requires((s, t)),
        forall|s: S, t: &T| stderr.requires((s, t)),
    ensures
        match result {
            Ok(value) => stdout.ensures((state, &value), out.0) && out.1 == Ok::<u8, T>(0),
            Err(error) => stderr.ensures((state, &error), out.0) && out.1 == Err::<u8, T>(error),
        },
{
    match result {
        Ok(value) => (stdout(state, &value), Ok(0)),
        Err(error) => (stderr(state, &error), Err(error)),
    }
}

/// 未采用诊断时调用方传入空操作；写诊断本身失败时保留原始 I/O 错误。
pub fn finish_business<T, F, E>(result: Result<u8, T>, diagnose: F)
    -> (out: Result<u8, E>)
    where F: FnOnce(T) -> Result<(), E>,
    requires forall|error: T| diagnose.requires((error,)),
    ensures match result {
        Ok(code) => out == Ok::<u8, E>(code),
        Err(error) => exists|written: Result<(), E>| #[trigger] diagnose.ensures((error,), written)
            && match written {
                Ok(()) => out == Ok::<u8, E>(2),
                Err(io) => out == Err::<u8, E>(io),
            },
    },
{
    match result {
        Ok(code) => Ok(code),
        Err(error) => match diagnose(error) {
            Ok(()) => Ok(2),
            Err(io) => Err(io),
        },
    }
}

/// 诊断匹配业务退出码或明确报告失败；信号及其他退出保留原状态供 stderr 使用。
pub fn diagnostics_exit(business: u8, status: Option<i32>) -> (out: Result<u8, Option<i32>>)
    ensures
        status == Some(business as i32) ==> out == Ok::<u8, Option<i32>>(business),
        status == Some(2i32) ==> out == Ok::<u8, Option<i32>>(2),
        status != Some(business as i32) && status != Some(2i32)
            ==> out == Err::<u8, Option<i32>>(status),
{
    match status {
        Some(code) if code == business as i32 => Ok(business),
        Some(2) => Ok(2),
        other => Err(other),
    }
}

}
