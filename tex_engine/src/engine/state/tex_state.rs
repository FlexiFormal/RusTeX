/*! Implementation of a plain TeX [`State`]. */
use crate::commands::primitives::{PRIMITIVES, PrimitiveCommands, PrimitiveIdentifier};
use crate::commands::{PrimitiveCommand, TeXCommand};
use crate::engine::fontsystem::Font;
use crate::engine::mouth::Mouth;
use crate::engine::state::{GroupType, State, StateChange, StateChangeTracker, StateStack};
use crate::engine::utils::outputs::Outputs;
use crate::engine::{EngineAux, EngineTypes};
use crate::tex::catcodes::{CategoryCode, CategoryCodeScheme};
use crate::tex::characters::Character;
use crate::tex::characters::CharacterMap;
use crate::tex::nodes::boxes::TeXBox;
use crate::tex::numerics::{MuSkip, Skip};
use crate::tex::tokens::control_sequences::CSNameMap;
use crate::tex::tokens::control_sequences::{CSHandler, CSName};
use crate::tex::tokens::token_lists::TokenList;
use crate::utils::HMap;

/// Default implementation of a plain TeX [`State`].
#[derive(Clone)]
pub struct DefaultState<ET: EngineTypes> {
    stack: StateStack<ET>,
    primitives: PrimitiveCommands<ET>,
    catcodes: CategoryCodeScheme<ET::Char>,
    sfcodes: <ET::Char as Character>::CharMap<u16>,
    lccodes: <ET::Char as Character>::CharMap<ET::Char>,
    uccodes: <ET::Char as Character>::CharMap<ET::Char>,
    mathcodes: <ET::Char as Character>::CharMap<u32>,
    delcodes: <ET::Char as Character>::CharMap<ET::Int>,
    primitive_ints: HMap<PrimitiveIdentifier, ET::Int>,
    primitive_dims: HMap<PrimitiveIdentifier, ET::Dim>,
    primitive_skips: HMap<PrimitiveIdentifier, Skip<ET::Dim>>,
    primitive_muskips: HMap<PrimitiveIdentifier, MuSkip<ET::MuDim>>,
    primitive_toks: HMap<PrimitiveIdentifier, TokenList<ET::Token>>,
    int_register: Vec<ET::Int>,
    dim_register: Vec<ET::Dim>,
    skip_register: Vec<Skip<ET::Dim>>,
    muskip_register: Vec<MuSkip<ET::MuDim>>,
    toks_register: Vec<TokenList<ET::Token>>,
    box_register: Vec<Option<TeXBox<ET>>>,
    pub commands: <ET::CSName as CSName<ET::Char>>::Map<TeXCommand<ET>>, //Vec<Option<Command<ET>>>,//HMap<<ET::Token as Token>::CS,Command<ET>>,
    ac_commands: <ET::Char as Character>::CharMap<Option<TeXCommand<ET>>>,
    endline_char: Option<ET::Char>,
    escape_char: Option<ET::Char>,
    newline_char: Option<ET::Char>,
    current_font: ET::Font,
    textfonts: [ET::Font; 16],
    scriptfonts: [ET::Font; 16],
    scriptscriptfonts: [ET::Font; 16],
    empty_list: TokenList<ET::Token>,
    parshape: Vec<(ET::Dim, ET::Dim)>,
    par_token: ET::CSName,
}
impl<ET: EngineTypes> DefaultState<ET> {
    fn tracing_assigns(&self) -> bool {
        matches!(self.primitive_ints.get(&PRIMITIVES.tracingassigns), Some(v) if *v > ET::Int::default())
    }
    fn tracing_restores(&self) -> bool {
        matches!(self.primitive_ints.get(&PRIMITIVES.tracingrestores), Some(v) if *v > ET::Int::default())
    }
    pub fn set_command_direct(&mut self, name: ET::CSName, cmd: Option<TeXCommand<ET>>) {
        match cmd {
            None => self.commands.remove(&name),
            Some(c) => self.commands.insert(name, c),
        };
    }
}

impl<ET: EngineTypes> StateChangeTracker<ET> for DefaultState<ET> {
    fn stack(&mut self) -> &mut StateStack<ET> {
        &mut self.stack
    }
}

impl<ET: EngineTypes> State<ET> for DefaultState<ET> {
    fn new(nullfont: ET::Font, aux: &mut EngineAux<ET>) -> Self {
        let mem = &aux.memory;
        let mut lccodes: <ET::Char as Character>::CharMap<ET::Char> = CharacterMap::default();
        let mut uccodes: <ET::Char as Character>::CharMap<ET::Char> = CharacterMap::default();
        let mut mathcodes: <ET::Char as Character>::CharMap<u32> = CharacterMap::default();
        for i in 97..123 {
            *uccodes.get_mut(i.into()) = (i - 32).into();
            *lccodes.get_mut((i - 32).into()) = i.into();
            *mathcodes.get_mut(ET::Char::from(i - 32)) =
                (i as u32 - 32) + (16 * 16) + (7 * 16 * 16 * 16);
            *mathcodes.get_mut(ET::Char::from(i)) = (i as u32) + (16 * 16) + (7 * 16 * 16 * 16);
        }
        for i in 48..58 {
            *mathcodes.get_mut(ET::Char::from(i)) = (i as u32) + (7 * 16 * 16 * 16);
        }
        let mathfonts = array_init::array_init(|_| nullfont.clone());
        let par = aux.memory.cs_interner().par();
        Self {
            stack: StateStack::default(),
            primitives: PrimitiveCommands::default(),
            catcodes: ET::Char::starting_catcode_scheme(),
            sfcodes: CharacterMap::default(),
            delcodes: CharacterMap::default(),
            lccodes,
            uccodes,
            mathcodes,
            current_font: nullfont,
            primitive_ints: HMap::default(),
            primitive_dims: HMap::default(),
            primitive_skips: HMap::default(),
            primitive_muskips: HMap::default(),
            primitive_toks: HMap::default(),
            int_register: Vec::new(),
            dim_register: Vec::new(),
            skip_register: Vec::new(),
            muskip_register: Vec::new(),
            toks_register: Vec::new(),
            box_register: Vec::new(),
            commands: <ET::CSName as CSName<ET::Char>>::Map::default(),
            ac_commands: <ET::Char as Character>::CharMap::default(),
            endline_char: Some(ET::Char::from(b'\r')),
            escape_char: Some(ET::Char::from(b'\\')),
            newline_char: None,
            empty_list: mem.empty_list(),
            parshape: Vec::new(),
            textfonts: mathfonts.clone(),
            scriptfonts: mathfonts.clone(),
            scriptscriptfonts: mathfonts,
            par_token: par,
        }
    }

    fn get_par_token(&self) -> <ET as EngineTypes>::CSName {
        self.par_token.clone()
    }
    fn set_par_token(&mut self, par: <ET as EngineTypes>::CSName) {
        self.par_token = par;
    }

    fn register_primitive(
        &mut self,
        aux: &mut EngineAux<ET>,
        name: &'static str,
        cmd: PrimitiveCommand<ET>,
    ) {
        let id = self.primitives.register(name, cmd.clone());
        let name = aux.memory.cs_interner_mut().cs_from_str(name);
        self.commands
            .insert(name, TeXCommand::Primitive { name: id, cmd });
    }
    fn primitives(&self) -> &PrimitiveCommands<ET> {
        &self.primitives
    }

    fn get_group_type(&self) -> Option<GroupType> {
        self.stack.stack.last().map(|lvl| lvl.group_type)
    }

    fn get_group_level(&self) -> usize {
        self.stack.stack.len()
    }

    fn aftergroup(&mut self, token: ET::Token) {
        match self.stack.stack.last_mut() {
            None => (),
            Some(lvl) => lvl.aftergroup.push(token),
        }
    }

    fn push(&mut self, aux: &mut EngineAux<ET>, group_type: GroupType, line_number: usize) {
        self.stack.push(group_type);
        let tracing = matches!(self.primitive_ints.get(&PRIMITIVES.tracinggroups), Some(v) if *v > ET::Int::default());
        if tracing {
            aux.outputs.write_neg1(format_args!(
                "{{entering {} group (level {}) at line {}}}",
                group_type,
                self.stack.stack.len(),
                line_number
            ))
        }
    }

    fn pop(&mut self, aux: &mut EngineAux<ET>, mouth: &mut ET::Mouth) {
        let len = self.stack.stack.len();
        assert!(len > 0);
        let traceg = matches!(self.primitive_ints.get(&PRIMITIVES.tracinggroups), Some(v) if *v > ET::Int::default());
        let trace = self.tracing_restores();

        let (gt, ag, ch) = self.stack.pop();

        if traceg {
            aux.outputs.write_neg1(format_args!(
                "{{leaving {} group (level {}) at line {}}}",
                gt,
                len,
                mouth.line_number()
            ))
        }
        ch.close(|c| {
            match c {
                //StateChange::Custom { change } => change.restore(aux,self,trace),
                StateChange::Catcode { char, old } => {
                    if trace {
                        aux.outputs.write_neg1(format_args!(
                            "{{restoring {}catcode{}={}}}",
                            <ET::Char as Character>::display_opt(self.escape_char),
                            char.into(),
                            old
                        ));
                    }
                    *self.catcodes.get_mut(char) = old;
                }
                StateChange::CurrentFont(font) => {
                    if trace {
                        aux.outputs.write_neg1(format_args!(
                            "{{restoring current font ={}{}}}",
                            <ET::Char as Character>::display_opt(self.escape_char),
                            aux.memory.cs_interner().resolve(font.name())
                        ));
                    }
                    self.current_font = font;
                }
                StateChange::TextFont { idx, old: font } => {
                    if trace {
                        aux.outputs.write_neg1(format_args!(
                            "{{restoring textfont{} ={}{}}}",
                            idx,
                            <ET::Char as Character>::display_opt(self.escape_char),
                            aux.memory.cs_interner().resolve(font.name())
                        ));
                    }
                    self.textfonts[idx as usize] = font;
                }
                StateChange::ScriptFont { idx, old: font } => {
                    if trace {
                        aux.outputs.write_neg1(format_args!(
                            "{{restoring scriptfont{} ={}{}}}",
                            idx,
                            <ET::Char as Character>::display_opt(self.escape_char),
                            aux.memory.cs_interner().resolve(font.name())
                        ));
                    }
                    self.scriptfonts[idx as usize] = font;
                }
                StateChange::ScriptScriptFont { idx, old: font } => {
                    if trace {
                        aux.outputs.write_neg1(format_args!(
                            "{{restoring scriptscriptfont{} ={}{}}}",
                            idx,
                            <ET::Char as Character>::display_opt(self.escape_char),
                            aux.memory.cs_interner().resolve(font.name())
                        ));
                    }
                    self.scriptscriptfonts[idx as usize] = font;
                }
                StateChange::ParShape { old } => {
                    if trace {
                        aux.outputs.write_neg1(format_args!("{{TODO parshape}}"));
                    }
                    self.parshape = old;
                }
                StateChange::SfCode { char, old } => {
                    if trace {
                        aux.outputs.write_neg1(format_args!(
                            "{{restoring {}sfcode{}={}}}",
                            <ET::Char as Character>::display_opt(self.escape_char),
                            char.into(),
                            old
                        ));
                    }
                    *self.sfcodes.get_mut(char) = old;
                }
                StateChange::DelCode { char, old } => {
                    if trace {
                        aux.outputs.write_neg1(format_args!(
                            "{{restoring {}delcode{}={}}}",
                            <ET::Char as Character>::display_opt(self.escape_char),
                            char.into(),
                            old
                        ));
                    }
                    *self.delcodes.get_mut(char) = old;
                }
                StateChange::LcCode { char, old } => {
                    if trace {
                        aux.outputs.write_neg1(format_args!(
                            "{{restoring {}lccode{}={}}}",
                            <ET::Char as Character>::display_opt(self.escape_char),
                            char.into(),
                            old
                        ));
                    }
                    *self.lccodes.get_mut(char) = old;
                }
                StateChange::UcCode { char, old } => {
                    if trace {
                        aux.outputs.write_neg1(format_args!(
                            "{{restoring {}uccode{}={}}}",
                            <ET::Char as Character>::display_opt(self.escape_char),
                            char.into(),
                            old
                        ));
                    }
                    *self.uccodes.get_mut(char) = old;
                }
                StateChange::MathCode { char, old } => {
                    if trace {
                        aux.outputs.write_neg1(format_args!(
                            "{{restoring {}mathcode{}=\"{:X}}}",
                            <ET::Char as Character>::display_opt(self.escape_char),
                            char.into(),
                            old
                        ));
                    }
                    *self.mathcodes.get_mut(char) = old;
                }
                StateChange::EndlineChar { old } => {
                    if trace {
                        aux.outputs.write_neg1(format_args!(
                            "{{restoring {}endlinechar={}}}",
                            <ET::Char as Character>::display_opt(self.escape_char),
                            match old {
                                None => -1,
                                Some(c) => c.into() as i64,
                            }
                        ));
                    }
                    self.endline_char = old;
                }
                StateChange::EscapeChar { old } => {
                    if trace {
                        aux.outputs.write_neg1(format_args!(
                            "{{restoring {}escapechar={}}}",
                            <ET::Char as Character>::display_opt(self.escape_char),
                            match old {
                                None => -1,
                                Some(c) => c.into() as i64,
                            }
                        ));
                    }
                    self.escape_char = old;
                }
                StateChange::NewlineChar { old } => {
                    if trace {
                        aux.outputs.write_neg1(format_args!(
                            "{{restoring {}newlinechar={}}}",
                            <ET::Char as Character>::display_opt(self.escape_char),
                            match old {
                                None => -1,
                                Some(c) => c.into() as i64,
                            }
                        ));
                    }
                    self.newline_char = old;
                }
                StateChange::IntRegister { idx, old } => {
                    if trace {
                        aux.outputs.write_neg1(format_args!(
                            "{{restoring {}count{}={}}}",
                            <ET::Char as Character>::display_opt(self.escape_char),
                            idx,
                            old
                        ));
                    }
                    self.int_register[idx] = old;
                }
                StateChange::DimRegister { idx, old } => {
                    if trace {
                        aux.outputs.write_neg1(format_args!(
                            "{{restoring {}dimen{}={}}}",
                            <ET::Char as Character>::display_opt(self.escape_char),
                            idx,
                            old
                        ));
                    }
                    self.dim_register[idx] = old;
                }
                StateChange::SkipRegister { idx, old } => {
                    if trace {
                        aux.outputs.write_neg1(format_args!(
                            "{{restoring {}skip{}={}}}",
                            <ET::Char as Character>::display_opt(self.escape_char),
                            idx,
                            old
                        ));
                    }
                    self.skip_register[idx] = old;
                }
                StateChange::MuSkipRegister { idx, old } => {
                    if trace {
                        aux.outputs.write_neg1(format_args!(
                            "{{restoring {}muskip{}={}}}",
                            <ET::Char as Character>::display_opt(self.escape_char),
                            idx,
                            old
                        ));
                    }
                    self.muskip_register[idx] = old;
                }
                StateChange::ToksRegister { idx, old } => {
                    if trace {
                        aux.outputs.write_neg1(format_args!(
                            "{{restoring {}toks{}={}}}",
                            <ET::Char as Character>::display_opt(self.escape_char),
                            idx,
                            old.display(
                                aux.memory.cs_interner(),
                                &self.catcodes,
                                self.escape_char,
                                false
                            )
                        ));
                    }
                    self.toks_register[idx] = old;
                }
                StateChange::BoxRegister { idx, old } => {
                    if trace {
                        aux.outputs.write_neg1("TODO trace box restore")
                    }
                    self.box_register[idx] = old;
                }
                StateChange::PrimitiveInt { name, old } => {
                    if trace {
                        aux.outputs.write_neg1(format_args!(
                            "{{restoring {}{}={}}}",
                            <ET::Char as Character>::display_opt(self.escape_char),
                            name.display(self.escape_char),
                            old
                        ));
                    }
                    self.primitive_ints.insert(name, old);
                }
                StateChange::PrimitiveDim { name, old } => {
                    if trace {
                        aux.outputs.write_neg1(format_args!(
                            "{{restoring {}{}={}}}",
                            <ET::Char as Character>::display_opt(self.escape_char),
                            name.display(self.escape_char),
                            old
                        ));
                    }
                    self.primitive_dims.insert(name, old);
                }
                StateChange::PrimitiveSkip { name, old } => {
                    if trace {
                        aux.outputs.write_neg1(format_args!(
                            "{{restoring {}{}={}}}",
                            <ET::Char as Character>::display_opt(self.escape_char),
                            name.display(self.escape_char),
                            old
                        ));
                    }
                    self.primitive_skips.insert(name, old);
                }
                StateChange::PrimitiveMuSkip { name, old } => {
                    if trace {
                        aux.outputs.write_neg1(format_args!(
                            "{{restoring {}{}={}}}",
                            <ET::Char as Character>::display_opt(self.escape_char),
                            name.display(self.escape_char),
                            old
                        ));
                    }
                    self.primitive_muskips.insert(name, old);
                }
                StateChange::PrimitiveToks { name, old } => {
                    if trace {
                        aux.outputs.write_neg1(format_args!(
                            "{{restoring {}{}={}}}",
                            <ET::Char as Character>::display_opt(self.escape_char),
                            name.display(self.escape_char),
                            old.display(
                                aux.memory.cs_interner(),
                                &self.catcodes,
                                self.escape_char,
                                false
                            )
                        ));
                    }
                    self.primitive_toks.insert(name, old);
                }
                StateChange::Command { name, old } => {
                    /*
                                        {
                                            let dpname = aux.memory.cs_interner().resolve(&name);
                                            let dpname = dpname.to_string();
                                            if dpname.starts_with("c_stex_module_") && dpname.ends_with("_notations_prop") {
                                                println!("\n\n Resetting {dpname} to {}",old.as_ref().map(|c| c.meaning(aux.memory.cs_interner(),&self.catcodes,self.escape_char).to_string()).unwrap_or_else(|| "(NONE)".to_string()));
                                            }
                                        }
                    */
                    if trace {
                        match old {
                            None => aux.outputs.write_neg1(format_args!(
                                "{{restoring {}{}={}undefined}}",
                                <ET::Char as Character>::display_opt(self.escape_char),
                                aux.memory.cs_interner().resolve(&name),
                                <ET::Char as Character>::display_opt(self.escape_char)
                            )),
                            Some(ref c) => aux.outputs.write_neg1(format_args!(
                                "{{restoring {}{}={}}}",
                                <ET::Char as Character>::display_opt(self.escape_char),
                                aux.memory.cs_interner().resolve(&name),
                                c.meaning(
                                    aux.memory.cs_interner(),
                                    &self.catcodes,
                                    self.escape_char
                                )
                            )),
                        }
                    }
                    match old {
                        Some(o) => self.commands.insert(name, o),
                        None => self.commands.remove(&name),
                    };
                }
                StateChange::AcCommand { char, old } => {
                    if trace {
                        match old {
                            None => aux.outputs.write_neg1(format_args!(
                                "{{restoring {}{}={}undefined}}",
                                <ET::Char as Character>::display_opt(self.escape_char),
                                char.display(),
                                <ET::Char as Character>::display_opt(self.escape_char)
                            )),
                            Some(ref c) => aux.outputs.write_neg1(format_args!(
                                "{{restoring {}{}={}}}",
                                <ET::Char as Character>::display_opt(self.escape_char),
                                char.display(),
                                c.meaning(
                                    aux.memory.cs_interner(),
                                    &self.catcodes,
                                    self.escape_char
                                )
                            )),
                        }
                    }
                    *self.ac_commands.get_mut(char) = old;
                } /*
                  StateChange::Custom {change:c} => {
                      let mut m = c.lock().unwrap().take();
                      if let Some(ref mut c) = m {
                          c.restore(aux,self,trace);
                      }
                  }*/
            }
        });
        mouth.push_vec(ag);
    }

    fn get_parshape(&self) -> &Vec<(ET::Dim, ET::Dim)> {
        &self.parshape
    }
    fn take_parshape(&mut self) -> Vec<(ET::Dim, ET::Dim)> {
        let sh = std::mem::take(&mut self.parshape);
        self.stack
            .add_change_locally(StateChange::ParShape { old: sh.clone() });
        sh
    }
    fn set_parshape(
        &mut self,
        aux: &EngineAux<ET>,
        parshape: Vec<(ET::Dim, ET::Dim)>,
        globally: bool,
    ) {
        self.change_field(globally, |s, g| {
            if s.tracing_assigns() {
                aux.outputs.write_neg1(format_args!(
                    "{{{}changing parshape}}",
                    if g { "globally " } else { "" }
                ));
            }
            let old = std::mem::replace(&mut s.parshape, parshape);
            StateChange::ParShape { old }
        })
    }

    fn get_current_font(&self) -> &ET::Font {
        &self.current_font
    }
    fn set_current_font(&mut self, aux: &mut EngineAux<ET>, fnt: ET::Font, globally: bool) {
        self.change_field(globally, |s, g| {
            if s.tracing_assigns() {
                aux.outputs.write_neg1(format_args!(
                    "{{{}changing current font={}{}}}",
                    if g { "globally " } else { "" },
                    <ET::Char as Character>::display_opt(s.escape_char),
                    aux.memory.cs_interner().resolve(s.current_font.name())
                ));
                aux.outputs.write_neg1(format_args!(
                    "{{into current font={}{}}}",
                    <ET::Char as Character>::display_opt(s.escape_char),
                    aux.memory.cs_interner().resolve(fnt.name())
                ));
            }
            let old = std::mem::replace(&mut s.current_font, fnt);
            StateChange::CurrentFont(old)
        })
    }

    fn get_textfont(&self, i: u8) -> &<ET as EngineTypes>::Font {
        &self.textfonts[i as usize]
    }
    fn set_textfont(
        &mut self,
        aux: &mut EngineAux<ET>,
        idx: u8,
        fnt: <ET as EngineTypes>::Font,
        globally: bool,
    ) {
        self.change_field(globally, |s, g| {
            if s.tracing_assigns() {
                aux.outputs.write_neg1(format_args!(
                    "{{{}changing textfont{}={}{}}}",
                    if g { "globally " } else { "" },
                    idx,
                    <ET::Char as Character>::display_opt(s.escape_char),
                    aux.memory
                        .cs_interner()
                        .resolve(s.textfonts[idx as usize].name())
                ));
                aux.outputs.write_neg1(format_args!(
                    "{{into textfont{}={}{}}}",
                    idx,
                    <ET::Char as Character>::display_opt(s.escape_char),
                    aux.memory.cs_interner().resolve(fnt.name())
                ));
            }
            let old = std::mem::replace(&mut s.textfonts[idx as usize], fnt);
            StateChange::TextFont { idx, old }
        })
    }

    fn get_scriptfont(&self, i: u8) -> &<ET as EngineTypes>::Font {
        &self.scriptfonts[i as usize]
    }
    fn set_scriptfont(
        &mut self,
        aux: &mut EngineAux<ET>,
        idx: u8,
        fnt: <ET as EngineTypes>::Font,
        globally: bool,
    ) {
        self.change_field(globally, |s, g| {
            if s.tracing_assigns() {
                aux.outputs.write_neg1(format_args!(
                    "{{{}changing scriptfont{}={}{}}}",
                    if g { "globally " } else { "" },
                    idx,
                    <ET::Char as Character>::display_opt(s.escape_char),
                    aux.memory
                        .cs_interner()
                        .resolve(s.scriptfonts[idx as usize].name())
                ));
                aux.outputs.write_neg1(format_args!(
                    "{{into scriptfont{}={}{}}}",
                    idx,
                    <ET::Char as Character>::display_opt(s.escape_char),
                    aux.memory.cs_interner().resolve(fnt.name())
                ));
            }
            let old = std::mem::replace(&mut s.scriptfonts[idx as usize], fnt);
            StateChange::ScriptFont { idx, old }
        })
    }

    fn get_scriptscriptfont(&self, i: u8) -> &<ET as EngineTypes>::Font {
        &self.scriptscriptfonts[i as usize]
    }
    fn set_scriptscriptfont(
        &mut self,
        aux: &mut EngineAux<ET>,
        idx: u8,
        fnt: <ET as EngineTypes>::Font,
        globally: bool,
    ) {
        self.change_field(globally, |s, g| {
            if s.tracing_assigns() {
                aux.outputs.write_neg1(format_args!(
                    "{{{}changing scriptscriptfont{}={}{}}}",
                    if g { "globally " } else { "" },
                    idx,
                    <ET::Char as Character>::display_opt(s.escape_char),
                    aux.memory
                        .cs_interner()
                        .resolve(s.scriptscriptfonts[idx as usize].name())
                ));
                aux.outputs.write_neg1(format_args!(
                    "{{into scriptscriptfont{}={}{}}}",
                    idx,
                    <ET::Char as Character>::display_opt(s.escape_char),
                    aux.memory.cs_interner().resolve(fnt.name())
                ));
            }
            let old = std::mem::replace(&mut s.scriptscriptfonts[idx as usize], fnt);
            StateChange::ScriptScriptFont { idx, old }
        })
    }

    fn get_catcode_scheme(&self) -> &CategoryCodeScheme<ET::Char> {
        &self.catcodes
    }
    fn set_catcode(&mut self, aux: &EngineAux<ET>, c: ET::Char, cc: CategoryCode, globally: bool) {
        self.change_field(globally, |s, g| {
            if s.tracing_assigns() {
                let num = c.into();
                let cc: u8 = cc.into();
                aux.outputs.write_neg1(format_args!(
                    "{{{} {}catcode{}={}}}",
                    if g {
                        "globally changing"
                    } else {
                        "reassigning"
                    },
                    <ET::Char as Character>::display_opt(s.escape_char),
                    num,
                    cc
                ));
            }
            let old = std::mem::replace(s.catcodes.get_mut(c), cc);
            StateChange::Catcode { char: c, old }
        })
    }

    fn get_sfcode(&self, c: ET::Char) -> u16 {
        *self.sfcodes.get(c)
    }
    fn set_sfcode(&mut self, aux: &EngineAux<ET>, c: ET::Char, val: u16, globally: bool) {
        self.change_field(globally, |s, g| {
            let old = std::mem::replace(s.sfcodes.get_mut(c), val);
            if s.tracing_assigns() {
                let num = c.into();
                aux.outputs.write_neg1(format_args!(
                    "{{{}changing {}sfcode{}={}}}",
                    if g { "globally " } else { "" },
                    <ET::Char as Character>::display_opt(s.escape_char),
                    num,
                    old
                ));
                aux.outputs.write_neg1(format_args!(
                    "{{into {}sfcode{}={}}}",
                    <ET::Char as Character>::display_opt(s.escape_char),
                    num,
                    val
                ));
            }
            StateChange::SfCode { char: c, old }
        })
    }

    fn get_delcode(&self, c: ET::Char) -> ET::Int {
        *self.delcodes.get(c)
    }
    fn set_delcode(&mut self, aux: &EngineAux<ET>, c: ET::Char, val: ET::Int, globally: bool) {
        self.change_field(globally, |s, g| {
            let old = std::mem::replace(s.delcodes.get_mut(c), val);
            if s.tracing_assigns() {
                let num = c.into();
                aux.outputs.write_neg1(format_args!(
                    "{{{}changing {}delcode{}={}}}",
                    if g { "globally " } else { "" },
                    <ET::Char as Character>::display_opt(s.escape_char),
                    num,
                    old
                ));
                aux.outputs.write_neg1(format_args!(
                    "{{into {}delcode{}={}}}",
                    <ET::Char as Character>::display_opt(s.escape_char),
                    num,
                    val
                ));
            }
            StateChange::DelCode { char: c, old }
        })
    }

    fn get_lccode(&self, c: ET::Char) -> ET::Char {
        *self.lccodes.get(c)
    }
    fn set_lccode(&mut self, aux: &EngineAux<ET>, c: ET::Char, val: ET::Char, globally: bool) {
        self.change_field(globally, |s, g| {
            let old = std::mem::replace(s.lccodes.get_mut(c), val);
            if s.tracing_assigns() {
                let num = c.into();
                aux.outputs.write_neg1(format_args!(
                    "{{{}changing {}lccode{}={}}}",
                    if g { "globally " } else { "" },
                    <ET::Char as Character>::display_opt(s.escape_char),
                    num,
                    old.into()
                ));
                aux.outputs.write_neg1(format_args!(
                    "{{into {}lccode{}={}}}",
                    <ET::Char as Character>::display_opt(s.escape_char),
                    num,
                    val.into()
                ));
            }
            StateChange::LcCode { char: c, old }
        })
    }

    fn get_uccode(&self, c: ET::Char) -> ET::Char {
        *self.uccodes.get(c)
    }
    fn set_uccode(&mut self, aux: &EngineAux<ET>, c: ET::Char, val: ET::Char, globally: bool) {
        self.change_field(globally, |s, g| {
            let old = std::mem::replace(s.uccodes.get_mut(c), val);
            if s.tracing_assigns() {
                let num = c.into();
                aux.outputs.write_neg1(format_args!(
                    "{{{}changing {}uccode{}={}}}",
                    if g { "globally " } else { "" },
                    <ET::Char as Character>::display_opt(s.escape_char),
                    num,
                    old.into()
                ));
                aux.outputs.write_neg1(format_args!(
                    "{{into {}uccode{}={}}}",
                    <ET::Char as Character>::display_opt(s.escape_char),
                    num,
                    val.into()
                ));
            }
            StateChange::UcCode { char: c, old }
        })
    }

    fn get_mathcode(&self, c: ET::Char) -> u32 {
        *self.mathcodes.get(c)
    }
    fn set_mathcode(&mut self, aux: &EngineAux<ET>, c: ET::Char, val: u32, globally: bool) {
        self.change_field(globally, |s, g| {
            let old = std::mem::replace(s.mathcodes.get_mut(c), val);
            if s.tracing_assigns() {
                let num = c.into();
                aux.outputs.write_neg1(format_args!(
                    "{{{}changing {}mathcode{}=\"{:X}}}",
                    if g { "globally " } else { "" },
                    <ET::Char as Character>::display_opt(s.escape_char),
                    num,
                    old
                ));
                aux.outputs.write_neg1(format_args!(
                    "{{into {}mathcode{}=\"{:X}}}",
                    <ET::Char as Character>::display_opt(s.escape_char),
                    num,
                    val
                ));
            }
            StateChange::MathCode { char: c, old }
        })
    }

    fn get_endline_char(&self) -> Option<ET::Char> {
        self.endline_char
    }
    fn set_endline_char(&mut self, aux: &EngineAux<ET>, c: Option<ET::Char>, globally: bool) {
        self.change_field(globally, |s, g| {
            if s.tracing_assigns() {
                aux.outputs.write_neg1(format_args!(
                    "{{{}changing {}endlinechar={}}}",
                    if g { "globally " } else { "" },
                    <ET::Char as Character>::display_opt(s.escape_char),
                    match s.endline_char {
                        None => -1,
                        Some(c) => c.into() as i64,
                    }
                ));
                aux.outputs.write_neg1(format_args!(
                    "{{into {}endlinechar={}}}",
                    <ET::Char as Character>::display_opt(s.escape_char),
                    match c {
                        None => -1,
                        Some(c) => c.into() as i64,
                    }
                ));
            }
            let old = std::mem::replace(&mut s.endline_char, c);
            StateChange::EndlineChar { old }
        })
    }

    fn get_escape_char(&self) -> Option<ET::Char> {
        self.escape_char
    }
    fn set_escape_char(&mut self, aux: &EngineAux<ET>, c: Option<ET::Char>, globally: bool) {
        self.change_field(globally, |s, g| {
            if s.tracing_assigns() {
                aux.outputs.write_neg1(format_args!(
                    "{{{}changing {}escapechar={}}}",
                    if g { "globally " } else { "" },
                    <ET::Char as Character>::display_opt(s.escape_char),
                    match s.escape_char {
                        None => -1,
                        Some(c) => c.into() as i64,
                    }
                ));
                aux.outputs.write_neg1(format_args!(
                    "{{into {}escapechar={}}}",
                    <ET::Char as Character>::display_opt(s.escape_char),
                    match c {
                        None => -1,
                        Some(c) => c.into() as i64,
                    }
                ));
            }
            let old = std::mem::replace(&mut s.escape_char, c);
            StateChange::EscapeChar { old }
        })
    }

    fn get_newline_char(&self) -> Option<ET::Char> {
        self.newline_char
    }
    fn set_newline_char(&mut self, aux: &EngineAux<ET>, c: Option<ET::Char>, globally: bool) {
        self.change_field(globally, |s, g| {
            if s.tracing_assigns() {
                aux.outputs.write_neg1(format_args!(
                    "{{{}changing {}newlinechar={}}}",
                    if g { "globally " } else { "" },
                    <ET::Char as Character>::display_opt(s.escape_char),
                    match s.newline_char {
                        None => -1,
                        Some(c) => c.into() as i64,
                    }
                ));
                aux.outputs.write_neg1(format_args!(
                    "{{into {}newlinechar={}}}",
                    <ET::Char as Character>::display_opt(s.escape_char),
                    match c {
                        None => -1,
                        Some(c) => c.into() as i64,
                    }
                ));
            }
            let old = std::mem::replace(&mut s.newline_char, c);
            StateChange::NewlineChar { old }
        })
    }

    fn get_primitive_int(&self, name: PrimitiveIdentifier) -> ET::Int {
        match self.primitive_ints.get(&name) {
            Some(i) => *i,
            _ => ET::Int::default(),
        }
    }
    fn set_primitive_int(
        &mut self,
        aux: &EngineAux<ET>,
        name: PrimitiveIdentifier,
        v: ET::Int,
        globally: bool,
    ) {
        self.change_field(globally, |s, g| {
            let old = s.primitive_ints.insert(name, v).unwrap_or_default();
            if s.tracing_assigns() {
                aux.outputs.write_neg1(format_args!(
                    "{{{}changing {}={}}}",
                    if g { "globally " } else { "" },
                    name.display(s.escape_char),
                    old
                ));
                aux.outputs.write_neg1(format_args!(
                    "{{into {}={}}}",
                    name.display(s.escape_char),
                    v
                ))
            }
            StateChange::PrimitiveInt { name, old }
        });
    }

    fn get_int_register(&self, idx: usize) -> ET::Int {
        match self.int_register.get(idx) {
            Some(i) => *i,
            _ => ET::Int::default(),
        }
    }
    fn set_int_register(&mut self, aux: &EngineAux<ET>, idx: usize, v: ET::Int, globally: bool) {
        self.change_field(globally, |s, g| {
            if s.int_register.len() <= idx {
                s.int_register.resize(idx + 1, ET::Int::default());
            }
            let old = std::mem::replace(&mut s.int_register[idx], v);
            if s.tracing_assigns() {
                aux.outputs.write_neg1(format_args!(
                    "{{{}changing {}count{}={}}}",
                    if g { "globally " } else { "" },
                    ET::Char::display_opt(s.escape_char),
                    idx,
                    old
                ));
                aux.outputs.write_neg1(format_args!(
                    "{{into {}count{}={}}}",
                    ET::Char::display_opt(s.escape_char),
                    idx,
                    v
                ))
            }
            StateChange::IntRegister { idx, old }
        });
    }

    fn get_dim_register(&self, idx: usize) -> ET::Dim {
        match self.dim_register.get(idx) {
            Some(i) => *i,
            _ => ET::Dim::default(),
        }
    }
    fn set_dim_register(&mut self, aux: &EngineAux<ET>, idx: usize, v: ET::Dim, globally: bool) {
        self.change_field(globally, |s, g| {
            if s.dim_register.len() <= idx {
                s.dim_register.resize(idx + 1, ET::Dim::default());
            }
            let old = std::mem::replace(&mut s.dim_register[idx], v);
            if s.tracing_assigns() {
                aux.outputs.write_neg1(format_args!(
                    "{{{}changing {}dimen{}={}}}",
                    if g { "globally " } else { "" },
                    ET::Char::display_opt(s.escape_char),
                    idx,
                    old
                ));
                aux.outputs.write_neg1(format_args!(
                    "{{into {}dimen{}={}}}",
                    ET::Char::display_opt(s.escape_char),
                    idx,
                    v
                ));
            }
            StateChange::DimRegister { idx, old }
        });
    }

    fn get_skip_register(&self, idx: usize) -> Skip<ET::Dim> {
        match self.skip_register.get(idx) {
            Some(i) => *i,
            _ => Skip::default(),
        }
    }
    fn set_skip_register(
        &mut self,
        aux: &EngineAux<ET>,
        idx: usize,
        v: Skip<ET::Dim>,
        globally: bool,
    ) {
        self.change_field(globally, |s, g| {
            if s.skip_register.len() <= idx {
                s.skip_register.resize(idx + 1, Skip::default());
            }
            let old = std::mem::replace(&mut s.skip_register[idx], v);
            if s.tracing_assigns() {
                aux.outputs.write_neg1(format_args!(
                    "{{{}changing {}skip{}={}}}",
                    if g { "globally " } else { "" },
                    ET::Char::display_opt(s.escape_char),
                    idx,
                    old
                ));
                aux.outputs.write_neg1(format_args!(
                    "{{into {}skip{}={}}}",
                    ET::Char::display_opt(s.escape_char),
                    idx,
                    v
                ))
            }
            StateChange::SkipRegister { idx, old }
        });
    }

    fn get_muskip_register(&self, idx: usize) -> MuSkip<ET::MuDim> {
        match self.muskip_register.get(idx) {
            Some(i) => *i,
            _ => MuSkip::default(),
        }
    }
    fn set_muskip_register(
        &mut self,
        aux: &EngineAux<ET>,
        idx: usize,
        v: MuSkip<ET::MuDim>,
        globally: bool,
    ) {
        self.change_field(globally, |s, g| {
            if s.muskip_register.len() <= idx {
                s.muskip_register.resize(idx + 1, MuSkip::default());
            }
            let old = std::mem::replace(&mut s.muskip_register[idx], v);
            if s.tracing_assigns() {
                aux.outputs.write_neg1(format_args!(
                    "{{{}changing {}muskip{}={}}}",
                    if g { "globally " } else { "" },
                    ET::Char::display_opt(s.escape_char),
                    idx,
                    old
                ));
                aux.outputs.write_neg1(format_args!(
                    "{{into {}muskip{}={}}}",
                    ET::Char::display_opt(s.escape_char),
                    idx,
                    v
                ))
            }
            StateChange::MuSkipRegister { idx, old }
        });
    }

    fn get_box_register(&self, idx: usize) -> Option<&TeXBox<ET>> {
        match self.box_register.get(idx) {
            None => None,
            Some(i) => i.as_ref(),
        }
    }
    fn get_box_register_mut(&mut self, idx: usize) -> Option<&mut TeXBox<ET>> {
        match self.box_register.get_mut(idx) {
            None => None,
            Some(i) => i.as_mut(),
        }
    }
    fn take_box_register(&mut self, idx: usize) -> Option<TeXBox<ET>> {
        match self.box_register.get_mut(idx) {
            None => None,
            Some(i) => std::mem::take(i),
        }
    }
    fn set_box_register(
        &mut self,
        aux: &EngineAux<ET>,
        idx: usize,
        v: Option<TeXBox<ET>>,
        globally: bool,
    ) {
        self.change_field(globally, |s, _| {
            if s.box_register.len() <= idx {
                s.box_register.resize(idx + 1, None);
            }
            let old = std::mem::replace(&mut s.box_register[idx], v);
            if s.tracing_assigns() {
                aux.outputs
                    .write_neg1(format_args!("{{TODO: trace box register change {}}}", idx));
            }
            StateChange::BoxRegister { idx, old }
        });
    }

    fn get_toks_register(&self, idx: usize) -> &TokenList<ET::Token> {
        match self.toks_register.get(idx) {
            Some(i) => i,
            _ => &self.empty_list,
        }
    }
    fn set_toks_register(
        &mut self,
        aux: &EngineAux<ET>,
        idx: usize,
        v: TokenList<ET::Token>,
        globally: bool,
    ) {
        self.change_field(globally, |s, g| {
            if s.toks_register.len() <= idx {
                s.toks_register.resize(idx + 1, s.empty_list.clone());
            }
            let trace = s.tracing_assigns();
            if trace {
                aux.outputs.write_neg1(format_args!(
                    "{{{}changing {}toks{}={}}}",
                    if g { "globally " } else { "" },
                    ET::Char::display_opt(s.escape_char),
                    idx,
                    s.toks_register[idx].display(
                        aux.memory.cs_interner(),
                        &s.catcodes,
                        s.escape_char,
                        false
                    )
                ));
            }
            let old = std::mem::replace(&mut s.toks_register[idx], v);
            if s.tracing_assigns() {
                aux.outputs.write_neg1(format_args!(
                    "{{into {}toks{}={}}}",
                    ET::Char::display_opt(s.escape_char),
                    idx,
                    s.toks_register[idx].display(
                        aux.memory.cs_interner(),
                        &s.catcodes,
                        s.escape_char,
                        false
                    )
                ))
            }
            StateChange::ToksRegister { idx, old }
        });
    }

    fn get_primitive_tokens(&self, name: PrimitiveIdentifier) -> &TokenList<ET::Token> {
        match self.primitive_toks.get(&name) {
            Some(i) => i,
            _ => &self.empty_list,
        }
    }
    fn set_primitive_tokens(
        &mut self,
        aux: &EngineAux<ET>,
        name: PrimitiveIdentifier,
        v: TokenList<ET::Token>,
        globally: bool,
    ) {
        self.change_field(globally, |s, g| {
            let old = s
                .primitive_toks
                .insert(name, v)
                .unwrap_or(s.empty_list.clone());
            if s.tracing_assigns() {
                aux.outputs.write_neg1(format_args!(
                    "{{{}changing {}={}}}",
                    if g { "globally " } else { "" },
                    name.display(s.escape_char),
                    old.display(aux.memory.cs_interner(), &s.catcodes, s.escape_char, false)
                ));
                aux.outputs.write_neg1(format_args!(
                    "{{into {}={}}}",
                    name.display(s.escape_char),
                    s.primitive_toks.get(&name).unwrap().display(
                        aux.memory.cs_interner(),
                        &s.catcodes,
                        s.escape_char,
                        false
                    )
                ))
            }
            StateChange::PrimitiveToks { name, old }
        });
    }

    fn get_primitive_dim(&self, name: PrimitiveIdentifier) -> ET::Dim {
        match self.primitive_dims.get(&name) {
            Some(i) => *i,
            _ => ET::Dim::default(),
        }
    }
    fn set_primitive_dim(
        &mut self,
        aux: &EngineAux<ET>,
        name: PrimitiveIdentifier,
        v: ET::Dim,
        globally: bool,
    ) {
        self.change_field(globally, |s, g| {
            let old = s.primitive_dims.insert(name, v).unwrap_or_default();
            if s.tracing_assigns() {
                aux.outputs.write_neg1(format_args!(
                    "{{{}changing {}={}}}",
                    if g { "globally " } else { "" },
                    name.display(s.escape_char),
                    old
                ));
                aux.outputs.write_neg1(format_args!(
                    "{{into {}={}}}",
                    name.display(s.escape_char),
                    v
                ))
            }
            StateChange::PrimitiveDim { name, old }
        });
    }

    fn get_primitive_skip(&self, name: PrimitiveIdentifier) -> Skip<ET::Dim> {
        match self.primitive_skips.get(&name) {
            Some(i) => *i,
            _ => Skip::default(),
        }
    }
    fn set_primitive_skip(
        &mut self,
        aux: &EngineAux<ET>,
        name: PrimitiveIdentifier,
        v: Skip<ET::Dim>,
        globally: bool,
    ) {
        self.change_field(globally, |s, g| {
            let old = s.primitive_skips.insert(name, v).unwrap_or_default();
            if s.tracing_assigns() {
                aux.outputs.write_neg1(format_args!(
                    "{{{}changing {}={}}}",
                    if g { "globally " } else { "" },
                    name.display(s.escape_char),
                    old
                ));
                aux.outputs.write_neg1(format_args!(
                    "{{into {}={}}}",
                    name.display(s.escape_char),
                    v
                ))
            }
            StateChange::PrimitiveSkip { name, old }
        });
    }

    fn get_primitive_muskip(&self, name: PrimitiveIdentifier) -> MuSkip<ET::MuDim> {
        match self.primitive_muskips.get(&name) {
            Some(i) => *i,
            _ => MuSkip::default(),
        }
    }
    fn set_primitive_muskip(
        &mut self,
        aux: &EngineAux<ET>,
        name: PrimitiveIdentifier,
        v: MuSkip<ET::MuDim>,
        globally: bool,
    ) {
        self.change_field(globally, |s, g| {
            let old = s.primitive_muskips.insert(name, v).unwrap_or_default();
            if s.tracing_assigns() {
                aux.outputs.write_neg1(format_args!(
                    "{{{}changing {}={}}}",
                    if g { "globally " } else { "" },
                    name.display(s.escape_char),
                    old
                ));
                aux.outputs.write_neg1(format_args!(
                    "{{into {}={}}}",
                    name.display(s.escape_char),
                    v
                ))
            }
            StateChange::PrimitiveMuSkip { name, old }
        });
    }

    fn get_command(&self, name: &ET::CSName) -> Option<&TeXCommand<ET>> {
        self.commands.get(name)
    }
    fn set_command(
        &mut self,
        aux: &EngineAux<ET>,
        name: ET::CSName,
        cmd: Option<TeXCommand<ET>>,
        globally: bool,
    ) {
        self.change_field(globally, |s, g| {
            /*
                       {
                           let dpname = aux.memory.cs_interner().resolve(&name);
                           let dpname = dpname.to_string();
                           if dpname.starts_with("c_stex_module_") && dpname.ends_with("_notations_prop") {
                               println!("\n\n Setting {dpname} to {} \n(globally: {g})\n",cmd.as_ref().map(|c| c.meaning(aux.memory.cs_interner(),&s.catcodes,s.escape_char).to_string()).unwrap_or_else(|| "(NONE)".to_string()));
                           }
                       }
            */
            let old = match cmd {
                None => {
                    let o = s.commands.remove(&name);
                    if s.tracing_assigns() {
                        match o {
                            None => aux.outputs.write_neg1(format_args!(
                                "{{{}changing {}{}={}undefined}}",
                                if g { "globally " } else { "" },
                                ET::Char::display_opt(s.escape_char),
                                aux.memory.cs_interner().resolve(&name),
                                ET::Char::display_opt(s.escape_char)
                            )),
                            Some(ref c) => aux.outputs.write_neg1(format_args!(
                                "{{{}changing {}{}={}}}",
                                if g { "globally " } else { "" },
                                ET::Char::display_opt(s.escape_char),
                                aux.memory.cs_interner().resolve(&name),
                                c.meaning(aux.memory.cs_interner(), &s.catcodes, s.escape_char)
                            )),
                        }
                        aux.outputs.write_neg1(format_args!(
                            "{{into {}{}={}undefined}}",
                            ET::Char::display_opt(s.escape_char),
                            aux.memory.cs_interner().resolve(&name),
                            ET::Char::display_opt(s.escape_char)
                        ));
                    }
                    o
                }
                Some(cmd) => {
                    if s.tracing_assigns() {
                        match s.commands.get(&name) {
                            None => aux.outputs.write_neg1(format_args!(
                                "{{{}changing {}{}={}undefined}}",
                                if g { "globally " } else { "" },
                                ET::Char::display_opt(s.escape_char),
                                aux.memory.cs_interner().resolve(&name),
                                ET::Char::display_opt(s.escape_char)
                            )),
                            Some(c) => aux.outputs.write_neg1(format_args!(
                                "{{{}changing {}{}={}}}",
                                if g { "globally " } else { "" },
                                ET::Char::display_opt(s.escape_char),
                                aux.memory.cs_interner().resolve(&name),
                                c.meaning(aux.memory.cs_interner(), &s.catcodes, s.escape_char)
                            )),
                        }
                        aux.outputs.write_neg1(format_args!(
                            "{{into {}{}={}}}",
                            ET::Char::display_opt(s.escape_char),
                            aux.memory.cs_interner().resolve(&name),
                            cmd.meaning(aux.memory.cs_interner(), &s.catcodes, s.escape_char)
                        ));
                    }
                    s.commands.insert(name.clone(), cmd)
                }
            };
            StateChange::Command { name, old }
        });
    }

    fn get_ac_command(&self, c: ET::Char) -> Option<&TeXCommand<ET>> {
        self.ac_commands.get(c).as_ref()
    }
    fn set_ac_command(
        &mut self,
        _aux: &EngineAux<ET>,
        c: ET::Char,
        cmd: Option<TeXCommand<ET>>,
        globally: bool,
    ) {
        self.change_field(globally, |s, _| {
            let old = std::mem::replace(s.ac_commands.get_mut(c), cmd);
            StateChange::AcCommand { char: c, old }
        });
    }
}
