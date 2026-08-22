from pathlib import Path
p=Path('crates/umasim/src/trainer/local_ramen_trainer.rs')
s=p.read_text()
for a,b in [('{.55}','{0.55}'),('{.15}','{0.15}'),('{.25}','{0.25}'),('f32*.8','f32*0.8'),('f32*.4','f32*0.4')]:
    s=s.replace(a,b)
p.write_text(s)
