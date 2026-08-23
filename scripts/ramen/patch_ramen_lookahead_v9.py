from pathlib import Path
p=Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s=p.read_text()
s=s.replace('use rand::prelude::StdRng;', 'use rand::{SeedableRng,prelude::StdRng};')
s=s.replace('rules::{calc_ramen_pt_gain,calc_region_bonus}', 'rules::{calc_ramen_pt_gain,calc_region_bonus,list_special_targets_for}')
s=s.replace(''' /// 当前拉面效果与本回合可用训练窗口的耦合价值。
 pub ramen_alignment_weight:f32
}''',''' /// 吃面前前向评估中，“吃面后最佳实际动作”相对“不吃面最佳动作”的增量权重。
 pub ramen_lookahead_weight:f32,
 /// 地区分身存在随机性时的事前采样数；只作用于状态副本，不读取真实吃面结果。
 pub ramen_lookahead_samples:usize
}''')
s=s.replace('great_cross_bonus:0.,ramen_alignment_weight:0.0', 'great_cross_bonus:0.,ramen_lookahead_weight:1.0,ramen_lookahead_samples:12')
s=s.replace('''   else if let Some(v)=token.strip_prefix("align"){local.ramen_alignment_weight=v.parse::<f32>()?/100.0}
''','''   else if let Some(v)=token.strip_prefix("align"){local.ramen_lookahead_weight=v.parse::<f32>()?/100.0}
   else if let Some(v)=token.strip_prefix("look"){local.ramen_lookahead_weight=v.parse::<f32>()?/100.0}
   else if let Some(v)=token.strip_prefix("samples"){local.ramen_lookahead_samples=v.parse()?}
''')
start=s.index(' fn ramen_alignment(&self,g:&RamenGame,region_id:usize)->Result<f32>{')
end=s.index('\n fn decide_ramen(&self,g:&RamenGame,a:&[RamenAction])', start)
new=''' fn best_action_score(&self,g:&RamenGame)->Result<f32>{
  let actions=g.list_actions()?;
  let(idx,out)=self.decide_train(g,&actions)?;
  // 守门返回单项 MAX；吃面通常不改变治病/休息等守门结论，因此不把 MAX 计入前向增量。
  if out.len()!=actions.len(){return Ok(0.0)}
  Ok(out.get(idx).map(|x|x.score).unwrap_or(0.0))
 }
 /// 在真正吃面前，用状态副本执行候选面并评估其事后最佳动作。
 /// 所有 region_id 走同一逻辑；不按人数、彩圈或拉面名称硬编码排序。
 fn ramen_lookahead(&self,g:&RamenGame,region_id:usize)->Result<f32>{
  if self.config.ramen_lookahead_weight<=0.0{return Ok(0.0)}
  let mut no_eat=g.clone();no_eat.stage=RamenStage::Train;no_eat.ramen.current_ramen=None;no_eat.ramen.clear_pending();
  let baseline=self.best_action_score(&no_eat)?;
  let targets=list_special_targets_for(&g.ramen,region_id)?
   .into_iter().min_by_key(|t|t.iter().sum::<i32>())
   .ok_or_else(||anyhow::anyhow!("拉面 {region_id} 没有合法诀窍方案"))?;
  let n=self.config.ramen_lookahead_samples.max(1);let mut total=0.0;
  for sample in 0..n{
   let mut preview=g.clone();preview.ramen.current_ramen=None;preview.ramen.pending_ramen=Some(region_id);preview.ramen.pending_special_targets=targets;
   // 种子只由吃面前已知状态、候选和样本编号构成；不会读取真实策略流的落点。
   let seed=(g.turn()as u64).wrapping_mul(0x9E3779B97F4A7C15)^(g.ramen.scenario_pt as u64).rotate_left(17)^((region_id as u64)<<32)^sample as u64;
   let mut rng=StdRng::seed_from_u64(seed);preview.ground_ramen_effects(&mut rng)?;preview.stage=RamenStage::Train;
   // decide_train 会用 calc_training_buff/value/failure 对全部五个训练和其他合法动作统一评分。
   total+=self.best_action_score(&preview)?;
  }
  Ok((total/n as f32-baseline)*self.config.ramen_lookahead_weight)
 }'''
s=s[:start]+new+s[end:]
old='''let(ck,rmj,great)=self.scenario_threshold_value(g,post)?;let align=self.ramen_alignment(g,region_id)?;o.score+=ck+rmj+great+align;o.add("scenario_checkpoint",ck);o.add("rmj_cross",rmj);o.add("great_cross",great);o.add("ramen_alignment",align)'''
new2='''let(ck,rmj,great)=self.scenario_threshold_value(g,post)?;let look=self.ramen_lookahead(g,region_id)?;o.score+=ck+rmj+great+look;o.add("scenario_checkpoint",ck);o.add("rmj_cross",rmj);o.add("great_cross",great);o.add("ramen_lookahead",look)'''
if old not in s: raise SystemExit('decide ramen tail not found')
s=s.replace(old,new2)
p.write_text(s)
