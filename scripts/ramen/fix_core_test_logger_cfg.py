from pathlib import Path

utils = Path("crates/umasim/src/utils.rs")
text = utils.read_text(encoding="utf-8")
needle = '''#[cfg(feature = "cli")]
pub fn init_test_logger(spec: &str) -> Result<()> {
    LOGGER_INIT.get_or_init(|| {
        let logger = flexi_logger::Logger::try_with_str(spec)
            .expect("log spec 解析失败")
            .format_for_stderr(log_format)
            .log_to_stderr() // ⚠️ 只 stderr，不写文件
            .start()
            .expect("flexi_logger start 失败");
        // LOGGER.set 可能失败（被其他线程抢先），但只要 start 成功，log crate 已被初始化
        let _ = LOGGER.set(Mutex::new(logger));
    });
    Ok(())
}
'''
replacement = needle + '''
/// Core-only 测试不携带 CLI 日志后端；保留同一公开签名，让测试无需按 feature
/// 重复分支。`log` facade 在未安装 logger 时会安全地丢弃记录。
#[cfg(all(test, not(feature = "cli")))]
pub fn init_test_logger(_spec: &str) -> Result<()> {
    Ok(())
}
'''
if text.count(needle) != 1:
    raise SystemExit("utils init_test_logger anchor mismatch")
utils.write_text(text.replace(needle, replacement), encoding="utf-8")

turn_flow = Path("crates/umasim/src/output/turn_flow.rs")
text = turn_flow.read_text(encoding="utf-8")
needle = '''        // 切回 info：第 31 回合的规则层 diag（效果）可见
        if let Some(logger) = crate::gamedata::LOGGER.get() {
            let handle = logger.lock().map_err(|_| anyhow::anyhow!("LOGGER 锁中毒"))?;
            let spec = flexi_logger::LogSpecification::try_from("info")?;
            handle.set_new_spec(spec);
        }
'''
replacement = '''        // 切回 info：第 31 回合的规则层 diag（效果）可见。Core-only 测试没有
        // flexi_logger/LOGGER，保持静默即可；测试验证的是流程与渲染，不依赖日志后端。
        #[cfg(feature = "cli")]
        if let Some(logger) = crate::gamedata::LOGGER.get() {
            let handle = logger.lock().map_err(|_| anyhow::anyhow!("LOGGER 锁中毒"))?;
            let spec = flexi_logger::LogSpecification::try_from("info")?;
            handle.set_new_spec(spec);
        }
'''
if text.count(needle) != 1:
    raise SystemExit("turn_flow logger anchor mismatch")
turn_flow.write_text(text.replace(needle, replacement), encoding="utf-8")
