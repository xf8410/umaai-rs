from pathlib import Path
p=Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s=p.read_text()
s=s.replace('pub great_cross_bonus:f32\n}', 'pub great_cross_bonus:f32,\n /// 当前拉面效果与本回合可用训练窗口的耦合价值。\n pub ramen_alignment_weight:f32\n}')
s=s.replace('probabilistic_hint:false,expected_fail:false,checkpoint_scale:0.,rmj_cross_bonus:0.,great_cross_bonus:0.\n', 'probabilistic_hint:false,expected_fail:false,checkpoint_scale:0.,rmj_cross_bonus:0.,great_cross_bonus:0.,ramen_alignment_weight:0.0\n')
s=s.replace('else if let Some(v)=token.strip_prefix("great"){local.great_cross_bonus=v.parse()?}\n', 'else if let Some(v)=token.strip_prefix("great"){local.great_cross_bonus=v.parse()?}\n   else if let Some(v)=token.strip_prefix("rpt"){policy.ramen_pt_weight=v.parse::<f32>()?/100.0}\n   else if let Some(v)=token.strip_prefix("align"){local.ramen_alignment_weight=v.parse::<f32>()?/100.0}\n')
old='''fn decide_ramen(&self,g:&RamenGame,a:&[RamenAction])->Result<(usize,Vec<RamenPolicyOutput>)>{
  let(_,mut out)=self.policy.decide_ramen(g,a)?;let risk=(g.ramen.feeling_stock.iter().sum::<i32>()-self.config.feeling_overflow_threshold).max(0)as f32;
  for(act,o)in a.iter().zip(out.iter_mut()){if act.ramen.is_some(){let pressure=risk*self.config.overflow_value;o.score+=pressure;o.add("local_stock_pressure",pressure);let y=(g.current_year()-1)as usize;let post=g.ramen.scenario_pt+calc_ramen_pt_gain(y,g.ramen.eat_count)?;let(ck,rmj,great)=self.scenario_threshold_value(g,post)?;o.score+=ck+rmj+great;o.add("scenario_checkpoint",ck);o.add("rmj_cross",rmj);o.add("great_cross",great)}}Ok((Self::choose(&out),out))
 }'''
new='''fn ramen_alignment(&self,g:&RamenGame,region_id:usize)->Result<f32>{
  if self.config.ramen_alignment_weight<=0.0{return Ok(0.0)}
  let d=RAMENDATA.get().ok_or_else(||anyhow::anyhow!("RAMENDATA 未初始化"))?;
  let region=d.ramen_region_effect.get(region_id).ok_or_else(||anyhow::anyhow!("地区效果缺失: {region_id}"))?;
  let mut best=0.0f32;
  for &t in &region.at_trains{
   if !(0..5).contains(&t){continue}let tr=t as usize;let buffs=g.calc_training_buff(tr)?;let v=g.calc_training_value(&buffs,tr)?;
   let raw=v.status_pt[..5].iter().sum::<i32>()as f32+v.status_pt[5]as f32*2.0;
   let people=g.distribution().get(tr).map(|x|x.len()).unwrap_or(0)as f32;
   let shining=g.shining_count(tr)as f32;
   best=best.max(raw+people*8.0+shining*35.0);
  }
  let effect=(region.xunlian+region.youqing+region.pt_bonus)as f32+region.hint_count as f32*10.0;
  Ok(best*effect*self.config.ramen_alignment_weight/100.0)
 }
 fn decide_ramen(&self,g:&RamenGame,a:&[RamenAction])->Result<(usize,Vec<RamenPolicyOutput>)>{
  let(_,mut out)=self.policy.decide_ramen(g,a)?;let risk=(g.ramen.feeling_stock.iter().sum::<i32>()-self.config.feeling_overflow_threshold).max(0)as f32;
  for(act,o)in a.iter().zip(out.iter_mut()){if let Some(region_id)=act.ramen{let pressure=risk*self.config.overflow_value;o.score+=pressure;o.add("local_stock_pressure",pressure);let y=(g.current_year()-1)as usize;let post=g.ramen.scenario_pt+calc_ramen_pt_gain(y,g.ramen.eat_count)?;let(ck,rmj,great)=self.scenario_threshold_value(g,post)?;let align=self.ramen_alignment(g,region_id)?;o.score+=ck+rmj+great+align;o.add("scenario_checkpoint",ck);o.add("rmj_cross",rmj);o.add("great_cross",great);o.add("ramen_alignment",align)}}Ok((Self::choose(&out),out))
 }'''
if old not in s: raise SystemExit('decide_ramen block not found')
s=s.replace(old,new)
p.write_text(s)
