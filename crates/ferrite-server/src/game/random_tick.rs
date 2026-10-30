#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_world::{BlockFlags,BlockInfo,Chunk};
    struct Info;
    impl BlockInfo for Info {
        fn flags(&self, id:u32)->BlockFlags {BlockFlags {air:id==0,..Default::default()}}
        fn randomly_ticking(&self,id:u32)->bool {id==u32::from(ferrite_data::blocks::default_state("grass_block").unwrap())}
    }
    #[test]
    fn non_ticking_sections_do_not_sample_and_ticking_sections_use_one_draw_per_pick() {
        let mut chunk=Chunk::new(-64,32,0,0,16,6).unwrap();
        let grass=u32::from(ferrite_data::blocks::default_state("grass_block").unwrap());
        chunk.sections_mut()[1].set_block(0,grass,&Info).unwrap();
        let mut draws=0;let mut events=Vec::new();
        sample_chunk(&chunk, 2,-1,3,|| {draws+=1;0}, |e|events.push(e));
        assert_eq!(draws,3);assert_eq!(events.len(),3);
        assert!(events.iter().all(|e| e.pos==(32,-48,-16) && e.state==grass && e.block_tick));
    }
}
