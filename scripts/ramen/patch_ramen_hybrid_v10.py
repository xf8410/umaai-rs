from pathlib import Path
p=Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s=p.read_text()
s=s.replace('''    /// 地区分身存在随机性时的事前采样数；只作用于状态副本，不读取真实吃面结果。
    pub ramen_lookahead_samples: usize,
''','''    /// 地区分身存在随机性时的事前采样数；只作用于状态副本，不读取真实吃面结果。
    pub ramen_lookahead_samples: usize,
    /// 积极吃面节奏：存在可制作面时，前向值只负责在面之间排序，不与“不吃”竞争。
    pub eager_eat: bool,
''')
s=s.replace('''            ramen_lookahead_samples: 12,
''','''            ramen_lookahead_samples: 12,
            eager_eat: false,
''')
s=s.replace('''            } else if token == "plain" {
''','''            } else if token == "eager" {
                local.eager_eat = true
            } else if token == "plain" {
''')
old='''        Ok((Self::choose(&out), out))
    }
}
impl Trainer<RamenGame> for LocalRamenTrainer {'''
new='''        // 吃不吃与吃哪碗分层：eager 模式下，只要 RamenSelect 已列出可制作面，
        // 就在这些面之间 argmax；不扩展 selected_regions，也不枚举年度其他地区。
        // 吃完后的 Train 阶段仍根据真实落地状态重新比较全部合法动作。
        let chosen = if self.config.eager_eat {
            a.iter()
                .zip(out.iter())
                .enumerate()
                .filter(|(_, (act, _))| act.ramen.is_some())
                .max_by(|(li, (_, l)), (ri, (_, r))| l.score.total_cmp(&r.score).then_with(|| ri.cmp(li)))
                .map(|(i, _)| i)
                .unwrap_or_else(|| Self::choose(&out))
        } else {
            Self::choose(&out)
        };
        Ok((chosen, out))
    }
}
impl Trainer<RamenGame> for LocalRamenTrainer {'''
if old not in s: raise SystemExit('decide_ramen tail not found')
s=s.replace(old,new)
p.write_text(s)
