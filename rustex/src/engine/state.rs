use crate::engine::commands::CLOSE_FONT;
use crate::engine::{CSName, Font, Types};
use tex_engine::commands::primitives::{PrimitiveCommands, PrimitiveIdentifier};
use tex_engine::commands::{PrimitiveCommand, TeXCommand};
use tex_engine::engine::mouth::Mouth;
use tex_engine::engine::state::{GroupType, State, StateChangeTracker, StateStack};
use tex_engine::engine::{EngineAux, EngineTypes, state};
use tex_engine::prelude::*;
use tex_engine::tex::catcodes::{CategoryCode, CategoryCodeScheme};
use tex_engine::tex::nodes::boxes::TeXBox;
use tex_engine::tex::numerics::{Dim32, Mu, MuSkip, Skip};
use tex_engine::tex::tokens::CompactToken;
use tex_engine::tex::tokens::Token;
use tex_engine::tex::tokens::control_sequences::CSNameVec;

#[derive(Clone)]
pub struct RusTeXState(state::tex_state::DefaultState<Types>);
impl RusTeXState {
    pub fn destruct(self) -> CSNameVec<u8, TeXCommand<Types>> {
        self.0.commands
    }
    pub fn set_command_direct(&mut self, name: CSName, cmd: Option<TeXCommand<Types>>) {
        self.0.set_command_direct(name, cmd)
    }
}
impl StateChangeTracker<Types> for RusTeXState {
    fn stack(&mut self) -> &mut StateStack<Types> {
        self.0.stack()
    }
}
impl State<Types> for RusTeXState {
    fn new(nullfont: Font, aux: &mut EngineAux<Types>) -> Self {
        Self(state::tex_state::DefaultState::new(nullfont, aux))
    }
    fn register_primitive(
        &mut self,
        aux: &mut EngineAux<Types>,
        name: &'static str,
        cmd: PrimitiveCommand<Types>,
    ) {
        self.0.register_primitive(aux, name, cmd);
    }

    fn get_par_token(&self) -> <Types as EngineTypes>::CSName {
        self.0.get_par_token()
    }
    fn set_par_token(&mut self, par: <Types as EngineTypes>::CSName) {
        self.0.set_par_token(par);
    }

    fn primitives(&self) -> &PrimitiveCommands<Types> {
        self.0.primitives()
    }

    fn aftergroup(&mut self, token: CompactToken) {
        self.0.aftergroup(token);
    }

    fn push(&mut self, aux: &mut EngineAux<Types>, group_type: GroupType, line_number: usize) {
        self.0.push(aux, group_type, line_number);
        aux.extension.push();
    }

    fn pop(&mut self, aux: &mut EngineAux<Types>, mouth: &mut <Types as EngineTypes>::Mouth) {
        self.0.pop(aux, mouth);
        let closefont = CompactToken::from_cs(aux.memory.cs_interner_mut().cs_from_str(CLOSE_FONT));
        for _ in 0..aux.extension.pop() {
            mouth.requeue(closefont);
        }
    }

    fn get_group_type(&self) -> Option<GroupType> {
        self.0.get_group_type()
    }

    fn get_group_level(&self) -> usize {
        self.0.get_group_level()
    }

    fn get_current_font(&self) -> &Font {
        self.0.get_current_font()
    }

    fn get_textfont(&self, i: u8) -> &<Types as EngineTypes>::Font {
        self.0.get_textfont(i)
    }

    fn set_textfont(
        &mut self,
        aux: &mut EngineAux<Types>,
        idx: u8,
        fnt: <Types as EngineTypes>::Font,
        globally: bool,
    ) {
        self.0.set_textfont(aux, idx, fnt, globally);
    }

    fn get_scriptfont(&self, i: u8) -> &<Types as EngineTypes>::Font {
        self.0.get_scriptfont(i)
    }

    fn set_scriptfont(
        &mut self,
        aux: &mut EngineAux<Types>,
        idx: u8,
        fnt: <Types as EngineTypes>::Font,
        globally: bool,
    ) {
        self.0.set_scriptfont(aux, idx, fnt, globally);
    }

    fn get_scriptscriptfont(&self, i: u8) -> &<Types as EngineTypes>::Font {
        self.0.get_scriptscriptfont(i)
    }

    fn set_scriptscriptfont(
        &mut self,
        aux: &mut EngineAux<Types>,
        idx: u8,
        fnt: <Types as EngineTypes>::Font,
        globally: bool,
    ) {
        self.0.set_scriptscriptfont(aux, idx, fnt, globally);
    }

    fn set_current_font(&mut self, aux: &mut EngineAux<Types>, fnt: Font, globally: bool) {
        self.0.set_current_font(aux, fnt, globally);
    }

    fn get_catcode_scheme(&self) -> &CategoryCodeScheme<u8> {
        self.0.get_catcode_scheme()
    }

    fn set_catcode(&mut self, aux: &EngineAux<Types>, c: u8, cc: CategoryCode, globally: bool) {
        self.0.set_catcode(aux, c, cc, globally);
    }

    fn get_sfcode(&self, c: u8) -> u16 {
        self.0.get_sfcode(c)
    }

    fn set_sfcode(&mut self, aux: &EngineAux<Types>, c: u8, sfcode: u16, globally: bool) {
        self.0.set_sfcode(aux, c, sfcode, globally);
    }

    fn get_lccode(&self, c: u8) -> u8 {
        self.0.get_lccode(c)
    }

    fn set_lccode(&mut self, aux: &EngineAux<Types>, c: u8, lccode: u8, globally: bool) {
        self.0.set_lccode(aux, c, lccode, globally);
    }

    fn get_uccode(&self, c: u8) -> u8 {
        self.0.get_uccode(c)
    }

    fn set_uccode(&mut self, aux: &EngineAux<Types>, c: u8, uccode: u8, globally: bool) {
        self.0.set_uccode(aux, c, uccode, globally);
    }

    fn get_delcode(&self, c: u8) -> i32 {
        self.0.get_delcode(c)
    }

    fn set_delcode(&mut self, aux: &EngineAux<Types>, c: u8, delcode: i32, globally: bool) {
        self.0.set_delcode(aux, c, delcode, globally);
    }

    fn get_mathcode(&self, c: u8) -> u32 {
        self.0.get_mathcode(c)
    }

    fn set_mathcode(&mut self, aux: &EngineAux<Types>, c: u8, mathcode: u32, globally: bool) {
        self.0.set_mathcode(aux, c, mathcode, globally);
    }

    fn get_endline_char(&self) -> Option<u8> {
        self.0.get_endline_char()
    }

    fn set_endline_char(&mut self, aux: &EngineAux<Types>, c: Option<u8>, globally: bool) {
        self.0.set_endline_char(aux, c, globally);
    }

    fn get_escape_char(&self) -> Option<u8> {
        self.0.get_escape_char()
    }

    fn set_escape_char(&mut self, aux: &EngineAux<Types>, c: Option<u8>, globally: bool) {
        self.0.set_escape_char(aux, c, globally);
    }

    fn get_newline_char(&self) -> Option<u8> {
        self.0.get_newline_char()
    }

    fn set_newline_char(&mut self, aux: &EngineAux<Types>, c: Option<u8>, globally: bool) {
        self.0.set_newline_char(aux, c, globally);
    }

    fn get_parshape(&self) -> &Vec<(Dim32, Dim32)> {
        self.0.get_parshape()
    }

    fn take_parshape(&mut self) -> Vec<(Dim32, Dim32)> {
        self.0.take_parshape()
    }

    fn set_parshape(
        &mut self,
        aux: &EngineAux<Types>,
        parshape: Vec<(Dim32, Dim32)>,
        globally: bool,
    ) {
        self.0.set_parshape(aux, parshape, globally);
    }

    fn get_int_register(&self, idx: usize) -> i32 {
        self.0.get_int_register(idx)
    }

    fn set_int_register(&mut self, aux: &EngineAux<Types>, idx: usize, v: i32, globally: bool) {
        self.0.set_int_register(aux, idx, v, globally);
    }

    fn get_primitive_int(&self, name: PrimitiveIdentifier) -> i32 {
        self.0.get_primitive_int(name)
    }

    fn set_primitive_int(
        &mut self,
        aux: &EngineAux<Types>,
        name: PrimitiveIdentifier,
        v: i32,
        globally: bool,
    ) {
        self.0.set_primitive_int(aux, name, v, globally);
    }

    fn get_dim_register(&self, idx: usize) -> Dim32 {
        self.0.get_dim_register(idx)
    }

    fn set_dim_register(&mut self, aux: &EngineAux<Types>, idx: usize, v: Dim32, globally: bool) {
        self.0.set_dim_register(aux, idx, v, globally);
    }

    fn get_skip_register(&self, idx: usize) -> Skip<Dim32> {
        self.0.get_skip_register(idx)
    }

    fn set_skip_register(
        &mut self,
        aux: &EngineAux<Types>,
        idx: usize,
        v: Skip<Dim32>,
        globally: bool,
    ) {
        self.0.set_skip_register(aux, idx, v, globally);
    }

    fn get_muskip_register(&self, idx: usize) -> MuSkip<Mu> {
        self.0.get_muskip_register(idx)
    }

    fn set_muskip_register(
        &mut self,
        aux: &EngineAux<Types>,
        idx: usize,
        v: MuSkip<Mu>,
        globally: bool,
    ) {
        self.0.set_muskip_register(aux, idx, v, globally);
    }
    fn get_toks_register(&self, idx: usize) -> &TokenList<CompactToken> {
        self.0.get_toks_register(idx)
    }
    fn set_toks_register(
        &mut self,
        aux: &EngineAux<Types>,
        idx: usize,
        v: TokenList<CompactToken>,
        globally: bool,
    ) {
        self.0.set_toks_register(aux, idx, v, globally);
    }

    fn get_box_register(&self, idx: usize) -> Option<&TeXBox<Types>> {
        self.0.get_box_register(idx)
    }

    fn get_box_register_mut(&mut self, idx: usize) -> Option<&mut TeXBox<Types>> {
        self.0.get_box_register_mut(idx)
    }

    fn take_box_register(&mut self, idx: usize) -> Option<TeXBox<Types>> {
        self.0.take_box_register(idx)
    }

    fn set_box_register(
        &mut self,
        aux: &EngineAux<Types>,
        idx: usize,
        v: Option<TeXBox<Types>>,
        globally: bool,
    ) {
        self.0.set_box_register(aux, idx, v, globally);
    }

    fn get_primitive_dim(&self, name: PrimitiveIdentifier) -> Dim32 {
        self.0.get_primitive_dim(name)
    }

    fn set_primitive_dim(
        &mut self,
        aux: &EngineAux<Types>,
        name: PrimitiveIdentifier,
        v: Dim32,
        globally: bool,
    ) {
        self.0.set_primitive_dim(aux, name, v, globally);
    }

    fn get_primitive_skip(&self, name: PrimitiveIdentifier) -> Skip<Dim32> {
        self.0.get_primitive_skip(name)
    }

    fn set_primitive_skip(
        &mut self,
        aux: &EngineAux<Types>,
        name: PrimitiveIdentifier,
        v: Skip<Dim32>,
        globally: bool,
    ) {
        self.0.set_primitive_skip(aux, name, v, globally);
    }

    fn get_primitive_muskip(&self, name: PrimitiveIdentifier) -> MuSkip<Mu> {
        self.0.get_primitive_muskip(name)
    }

    fn set_primitive_muskip(
        &mut self,
        aux: &EngineAux<Types>,
        name: PrimitiveIdentifier,
        v: MuSkip<Mu>,
        globally: bool,
    ) {
        self.0.set_primitive_muskip(aux, name, v, globally);
    }

    fn get_primitive_tokens(&self, name: PrimitiveIdentifier) -> &TokenList<CompactToken> {
        self.0.get_primitive_tokens(name)
    }

    fn set_primitive_tokens(
        &mut self,
        aux: &EngineAux<Types>,
        name: PrimitiveIdentifier,
        v: TokenList<CompactToken>,
        globally: bool,
    ) {
        self.0.set_primitive_tokens(aux, name, v, globally);
    }

    fn get_command(&self, name: &CSName) -> Option<&TeXCommand<Types>> {
        self.0.get_command(name)
    }

    fn set_command(
        &mut self,
        aux: &EngineAux<Types>,
        name: CSName,
        cmd: Option<TeXCommand<Types>>,
        globally: bool,
    ) {
        self.0.set_command(aux, name, cmd, globally);
    }

    fn get_ac_command(&self, c: u8) -> Option<&TeXCommand<Types>> {
        self.0.get_ac_command(c)
    }

    fn set_ac_command(
        &mut self,
        aux: &EngineAux<Types>,
        c: u8,
        cmd: Option<TeXCommand<Types>>,
        globally: bool,
    ) {
        self.0.set_ac_command(aux, c, cmd, globally);
    }
}
