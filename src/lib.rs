#![feature(conservative_impl_trait)]

#[macro_use]
extern crate fuzzy_pickles_derive;

#[macro_use]
extern crate peresil;

extern crate unicode_xid;

pub mod tokenizer;

use std::collections::BTreeSet;
use unicode_xid::UnicodeXID;
use peresil::combinators::*;

use tokenizer::{Token, Tokens};

// define what you want to parse; likely a string
// create an error type
// definte type aliases
type Point<'s> = peresil::StringPoint<'s>;
type Master<'s> = peresil::ParseMaster<Point<'s>, Error, State>;
type Progress<'s, T> = peresil::Progress<Point<'s>, T, Error>;

#[derive(Debug, Default)]
struct State {
    // Constructs like `if expr {}` will greedily match `StructName
    // {}` as a structure literal expression and then fail because the
    // body of the `if` isn't found. In these contexts, we disable
    // struct literals. You can re-enable them by entering something
    // like parenthesis or a block.
    ignore_struct_literals: bool,
}

// define an error type - emphasis on errors. Need to implement Recoverable (more to discuss.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Error {
    Literal(&'static str),
    ExpectedIdentifier,
    ExpectedNumber,
    ExpectedKeyword,
    UnterminatedRawString,
}

impl peresil::Recoverable for Error {
    fn recoverable(&self) -> bool { true }
}

#[derive(Debug, PartialEq)]
pub struct ErrorDetail {
    location: usize,
    errors: BTreeSet<Error>,
}

impl ErrorDetail {
    pub fn with_text<'a>(&'a self, text: &'a str) -> ErrorDetailText<'a> {
        ErrorDetailText { detail: self, text }
    }
}

#[derive(Debug)]
pub struct ErrorDetailText<'a> {
    detail: &'a ErrorDetail,
    text: &'a str,
}

use std::fmt;

impl<'a> fmt::Display for ErrorDetailText<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let (head, tail) = self.text.split_at(self.detail.location);
        let start_of_line = head.rfind("\n").unwrap_or(0);
        let end_of_line = tail.find("\n").unwrap_or_else(|| tail.len());

        let head_line = &head[start_of_line..];
        let tail_line = &tail[..end_of_line];

        let line = head.matches("\n").count() + 1; // Normally the first line is #1, so add one
        let col = head_line.len();

        writeln!(f, "Unable to parse text (line {}, column {})", line, col)?;
        writeln!(f, "{}{}", head_line, tail_line)?;
        writeln!(f, "{:>width$}", "^", width = col)?;
        writeln!(f, "Expected:")?;
        for e in &self.detail.errors {
            writeln!(f, "  {:?}", e)?; // TODO: should be Display
        }
        Ok(())
    }
}

// Construct a point, initialize  the master. This is what stores errors
// todo: rename?

pub fn parse_rust_file(file: &str) -> Result<File, ErrorDetail> {
    let mut pt = Point::new(file);
    let mut pm = Master::with_state(State::default());
    let mut items = Vec::new();

    loop {
        let next_pt;

        let item = item(&mut pm, pt);
        let item = pm.finish(item);

        match item.status {
            peresil::Status::Success(s) => {
                items.push(s);
                next_pt = item.point;
            },
            peresil::Status::Failure(e) => {
                return Err(ErrorDetail {
                    location: item.point.offset,
                    errors: e.into_iter().collect(),
                })
            },
        }

        if next_pt.offset <= pt.offset {
            let end = std::cmp::min(pt.offset + 10, file.len());
            panic!("Could not make progress: {}...", &file[pt.offset..end]);
        }
        pt = next_pt;

        if pt.s.is_empty() { break }
    }

    Ok(File { items: items })

    // TODO: add `expect` to progress?
}

// TODO: enum variants track whole extent, enum delegates

pub type Extent = (usize, usize);

#[derive(Debug, Visit)]
pub struct File {
    items: Vec<Item>,
}

#[derive(Debug, Visit, Decompose)]
pub enum Item {
    Attribute(Attribute),
    Const(Const),
    Enum(Enum),
    ExternCrate(Crate),
    ExternBlock(ExternBlock),
    Function(Function),
    Impl(Impl),
    MacroCall(MacroCall),
    Module(Module),
    Static(Static),
    Struct(Struct),
    Trait(Trait),
    TypeAlias(TypeAlias),
    Use(Use),
    Whitespace(Vec<Whitespace>),
}

impl Item {
    #[allow(dead_code)]
    pub fn extent(&self) -> Extent {
        match *self {
            Item::Attribute(Attribute { extent, .. })     |
            Item::Const(Const { extent, .. })             |
            Item::Enum(Enum { extent, .. })               |
            Item::ExternCrate(Crate { extent, .. })       |
            Item::ExternBlock(ExternBlock { extent, .. }) |
            Item::Function(Function { extent, .. })       |
            Item::Impl(Impl { extent, .. })               |
            Item::MacroCall(MacroCall { extent, .. })     |
            Item::Module(Module { extent, .. })           |
            Item::Static(Static { extent, .. })           |
            Item::Struct(Struct { extent, .. })           |
            Item::Trait(Trait { extent, .. })             |
            Item::TypeAlias(TypeAlias { extent, .. })     |
            Item::Use(Use { extent, .. })                 => extent,
            Item::Whitespace(..)                          => unimplemented!(),
        }
    }
}

#[derive(Debug, Visit)]
pub struct Attribute {
    extent: Extent,
    is_containing: Option<Extent>,
    text: Extent,
}

#[derive(Debug, Visit)]
pub struct Lifetime {
    extent: Extent,
    name: Ident,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit, Decompose)]
pub enum Whitespace {
    Comment(Comment),
    Whitespace(Extent),
}

#[derive(Debug, Visit)]
pub struct Comment {
    extent: Extent,
    text: Extent,
}

#[derive(Debug, Visit)]
pub struct Use {
    extent: Extent,
    visibility: Option<Visibility>,
    path: Vec<Ident>,
    tail: UseTail,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit, Decompose)]
pub enum UseTail {
    Ident(UseTailIdent),
    Glob(UseTailGlob),
    Multi(UseTailMulti),
}

#[derive(Debug, Visit)]
pub struct UseTailIdent {
    extent: Extent,
    name: Ident,
    rename: Option<Ident>,
}

#[derive(Debug, Visit)]
pub struct UseTailGlob {
    extent: Extent,
}

#[derive(Debug, Visit)]
pub struct UseTailMulti {
    extent: Extent,
    names: Vec<UseTailIdent>,
}

#[derive(Debug, Visit)]
pub struct Function {
    pub extent: Extent,
    pub header: FunctionHeader,
    body: Block,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct FunctionHeader {
    pub extent: Extent,
    visibility: Option<Visibility>,
    qualifiers: FunctionQualifiers,
    pub name: Ident,
    generics: Option<GenericDeclarations>,
    arguments: Vec<Argument>,
    return_type: Option<Type>,
    wheres: Vec<Where>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct FunctionQualifiers {
    pub extent: Extent,
    is_const: Option<Extent>,
    is_unsafe: Option<Extent>,
    is_extern: Option<Extent>,
    abi: Option<String>,
}

#[derive(Debug, Visit)]
pub struct TraitImplFunctionHeader {
    extent: Extent,
    visibility: Option<Visibility>,
    qualifiers: FunctionQualifiers,
    pub name: Ident,
    generics: Option<GenericDeclarations>,
    arguments: Vec<TraitImplArgument>,
    return_type: Option<Type>,
    wheres: Vec<Where>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct GenericDeclarations {
    pub extent: Extent,
    lifetimes: Vec<GenericDeclarationLifetime>,
    types: Vec<GenericDeclarationType>,
}

#[derive(Debug, Visit)]
pub struct GenericDeclarationLifetime {
    extent: Extent,
    attributes: Vec<Attribute>,
    name: Lifetime,
    bounds: Vec<Lifetime>,
}

#[derive(Debug, Visit)]
pub struct GenericDeclarationType {
    extent: Extent,
    attributes: Vec<Attribute>,
    name: Ident,
    bounds: Option<GenericDeclarationTypeAddition>,
}

#[derive(Debug, Visit, Decompose)]
pub enum GenericDeclarationTypeAddition {
    Bounds(TraitBounds),
    Default(Type),
}

#[derive(Debug, Visit, Decompose)]
pub enum Type {
    Array(TypeArray),
    Combination(TypeCombination),
    Function(TypeFunction),
    Pointer(TypePointer),
    Reference(TypeReference),
    Slice(TypeSlice),
    Tuple(TypeTuple),
    Uninhabited(Extent),
}

impl Type {
    pub fn extent(&self) -> Extent {
        match *self {
            Type::Array(TypeArray { extent, .. })             |
            Type::Combination(TypeCombination { extent, .. }) |
            Type::Function(TypeFunction { extent, .. })       |
            Type::Pointer(TypePointer { extent, .. })         |
            Type::Reference(TypeReference { extent, .. })     |
            Type::Slice(TypeSlice { extent, .. })             |
            Type::Tuple(TypeTuple { extent, .. })             => extent,
            Type::Uninhabited(extent) => extent,
        }
    }
}

#[derive(Debug, Visit)]
pub struct TypeReference {
    extent: Extent,
    kind: TypeReferenceKind,
    typ: Box<Type>,
}

#[derive(Debug, Visit)]
pub struct TypeReferenceKind {
    extent: Extent,
    lifetime: Option<Lifetime>,
    mutable: Option<Extent>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct TypePointer {
    extent: Extent,
    kind: TypePointerKind,
    typ: Box<Type>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug)]
pub enum TypePointerKind {
    Const,
    Mutable,
}

#[derive(Debug, Visit)]
pub struct TypeArray {
    extent: Extent,
    typ: Box<Type>,
    count: Box<Expression>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct TypeHigherRankedTraitBounds {
    extent: Extent,
    lifetimes: Vec<Lifetime>,
    child: TypeHigherRankedTraitBoundsChild,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit, Decompose)]
pub enum TypeHigherRankedTraitBoundsChild {
    Named(TypeNamed),
    Function(TypeFunction),
    Reference(TypeReference),
}

#[derive(Debug, Visit)]
pub struct TypeImplTrait {
    extent: Extent,
    name: TypeNamed,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct TypeCombination {
    extent: Extent,
    base: TypeCombinationBase,
    additional: Vec<TypeCombinationAdditional>,
}

#[derive(Debug, Visit, Decompose)]
pub enum TypeCombinationBase {
    Disambiguation(TypeDisambiguation),
    HigherRankedTraitBounds(TypeHigherRankedTraitBounds),
    ImplTrait(TypeImplTrait),
    Named(TypeNamed),
}

#[derive(Debug, Visit, Decompose)]
pub enum TypeCombinationAdditional {
    Named(TypeNamed),
    Lifetime(Lifetime),
}

#[derive(Debug, Visit)]
pub struct TypeNamed {
    extent: Extent,
    path: Vec<TypeNamedComponent>,
}

#[derive(Debug, Visit)]
pub struct TypeNamedComponent {
    extent: Extent,
    ident: Ident,
    generics: Option<TypeGenerics>,
}

#[derive(Debug, Visit)]
pub struct TypeDisambiguation {
    extent: Extent,
    from_type: Box<Type>,
    to_type: Option<Box<TypeNamed>>,
    path: Vec<TypeNamedComponent>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct TypeSlice {
    extent: Extent,
    typ: Box<Type>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct TypeTuple {
    extent: Extent,
    types: Vec<Type>,
}

#[derive(Debug, Visit, Decompose)]
pub enum TypeGenerics {
    Function(TypeGenericsFunction),
    Angle(TypeGenericsAngle),
}

#[derive(Debug, Visit)]
pub struct TypeGenericsFunction {
    extent: Extent,
    types: Vec<Type>,
    return_type: Option<Box<Type>>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct TypeGenericsAngle {
    extent: Extent,
    members: Vec<TypeGenericsAngleMember>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit, Decompose)]
pub enum TypeGenericsAngleMember {
    Lifetime(Lifetime),
    Type(Type),
    AssociatedType(AssociatedType)
}

#[derive(Debug, Visit)]
pub struct AssociatedType {
    extent: Extent,
    name: Ident,
    value: Type,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct TypeFunction {
    extent: Extent,
    qualifiers: FunctionQualifiers,
    arguments: Vec<TraitImplArgument>, // TODO: rename this indicating it doesn't require names
    return_type: Option<Box<Type>>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Copy, Clone, Visit)]
pub struct Ident {
    pub extent: Extent,
}

// TODO: Can we reuse the path from the `use` statement?
#[derive(Debug, Visit)]
pub struct Path {
    extent: Extent,
    components: Vec<Ident>,
}

// TODO: Can we reuse the path from the `use` statement?
#[derive(Debug, Visit)]
pub struct PathedIdent {
    extent: Extent,
    components: Vec<PathComponent>,
}

#[derive(Debug, Visit)]
pub struct PathComponent {
    extent: Extent,
    ident: Ident,
    turbofish: Option<Turbofish>,
}

#[derive(Debug, Visit)]
pub struct Turbofish {
    extent: Extent,
    types: Vec<Type>,
}

impl From<Ident> for PathedIdent {
    fn from(other: Ident) -> PathedIdent {
        PathedIdent { extent: other.extent, components: vec![
            PathComponent { extent: other.extent, ident: other, turbofish: None },
        ] }
    }
}

#[derive(Debug, Visit)]
pub struct Const {
    extent: Extent,
    visibility: Option<Visibility>,
    name: Ident,
    typ: Type,
    value: Expression,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct Static {
    extent: Extent,
    visibility: Option<Visibility>,
    is_mut: Option<Extent>,
    name: Ident,
    typ: Type,
    value: Expression,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct Struct {
    pub extent: Extent,
    visibility: Option<Visibility>,
    name: Ident,
    generics: Option<GenericDeclarations>,
    wheres: Vec<Where>,
    body: StructDefinitionBody,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit, Decompose)]
pub enum StructDefinitionBody {
    Brace(StructDefinitionBodyBrace),
    Tuple(StructDefinitionBodyTuple),
    Empty(Extent),
}

#[derive(Debug, Visit)]
pub struct StructDefinitionBodyBrace {
    pub extent: Extent,
    fields: Vec<StructDefinitionFieldNamed>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct StructDefinitionFieldNamed {
    extent: Extent,
    attributes: Vec<Attribute>,
    visibility: Option<Visibility>,
    name: Ident,
    typ: Type,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct StructDefinitionBodyTuple {
    pub extent: Extent,
    fields: Vec<StructDefinitionFieldUnnamed>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct StructDefinitionFieldUnnamed {
    extent: Extent,
    attributes: Vec<Attribute>,
    visibility: Option<Visibility>,
    typ: Type,
}

#[derive(Debug, Visit)]
pub struct Enum {
    pub extent: Extent,
    visibility: Option<Visibility>,
    name: Ident,
    generics: Option<GenericDeclarations>,
    wheres: Vec<Where>,
    variants: Vec<EnumVariant>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct EnumVariant {
    extent: Extent,
    attributes: Vec<Attribute>,
    name: Ident,
    body: EnumVariantBody,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit, Decompose)]
pub enum EnumVariantBody {
    Tuple(Vec<StructDefinitionFieldUnnamed>),
    Struct(StructDefinitionBodyBrace),
    Unit(Option<Expression>),
}

#[derive(Debug, Visit, Decompose)]
pub enum Argument {
    SelfArgument(SelfArgument),
    Named(NamedArgument),
}

#[derive(Debug, Visit, Decompose)]
pub enum SelfArgument {
    Longhand(SelfArgumentLonghand),
    Shorthand(SelfArgumentShorthand),
}

#[derive(Debug, Visit)]
pub struct SelfArgumentLonghand {
    extent: Extent,
    name: Ident,
    typ: Type,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct SelfArgumentShorthand {
    extent: Extent,
    qualifier: Option<SelfArgumentShorthandQualifier>,
    name: Ident,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit, Decompose)]
pub enum SelfArgumentShorthandQualifier {
    Reference(TypeReferenceKind),
    Mut(Extent),
}

#[derive(Debug, Visit)]
pub struct NamedArgument {
    name: Pattern,
    typ: Type,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit, Decompose)]
pub enum TraitImplArgument {
    SelfArgument(SelfArgument),
    Named(TraitImplArgumentNamed),
}

#[derive(Debug, Visit)]
pub struct TraitImplArgumentNamed {
    name: Option<Pattern>,
    typ: Type,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit, Decompose)]
pub enum Where {
    Lifetime(WhereLifetime),
    Type(WhereType),
}

impl Where {
    pub fn extent(&self) -> Extent {
        match *self {
            Where::Lifetime(WhereLifetime { extent, .. }) |
            Where::Type(WhereType { extent, .. })         => extent,
        }
    }
}

#[derive(Debug, Visit)]
pub struct WhereLifetime {
    pub extent: Extent,
    name: Lifetime,
    bounds: Vec<Lifetime>,
}

#[derive(Debug, Visit)]
pub struct WhereType {
    pub extent: Extent,
    name: Type,
    bounds: TraitBounds,
}

#[derive(Debug, Visit)]
pub struct TraitBounds {
    pub extent: Extent,
    types: Vec<TraitBound>,
}

#[derive(Debug, Visit, Decompose)]
pub enum TraitBound {
    Lifetime(TraitBoundLifetime),
    Normal(TraitBoundNormal),
    Relaxed(TraitBoundRelaxed),
}

#[derive(Debug, Visit)]
pub struct TraitBoundLifetime {
    pub extent: Extent,
    lifetime: Lifetime,
}

#[derive(Debug, Visit)]
pub struct TraitBoundNormal {
    pub extent: Extent,
    typ: TraitBoundType,
}

#[derive(Debug, Visit)]
pub struct TraitBoundRelaxed {
    pub extent: Extent,
    typ: TraitBoundType,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit, Decompose)]
pub enum TraitBoundType {
    Named(TypeNamed),
    // TODO: HRTB Trait bounds don't really allow references or fn types, just named
    // We need to create a smaller enum here
    HigherRankedTraitBounds(TypeHigherRankedTraitBounds),
}

#[derive(Debug, Visit)]
pub struct Block {
    extent: Extent,
    statements: Vec<Statement>,
    expression: Option<Expression>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct UnsafeBlock {
    extent: Extent,
    body: Box<Block>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct Parenthetical {
    extent: Extent,
    expression: Box<Expression>,
}

#[derive(Debug, Visit, Decompose)]
pub enum Statement {
    Expression(Expression),
    Item(Item),
}

impl Statement {
    #[allow(dead_code)]
    pub fn extent(&self) -> Extent {
        use Statement::*;
        match *self {
            Expression(ref e) => e.extent(),
            Item(ref i) => i.extent(),
        }
    }
}

#[derive(Debug, Visit, Decompose)]
pub enum Expression {
    Array(Array),
    AsType(AsType),
    Binary(Binary),
    Block(Box<Block>),
    Box(ExpressionBox),
    Break(Break),
    Byte(Byte),
    ByteString(ByteString),
    Call(Call),
    Character(Character),
    Closure(Closure),
    Continue(Continue),
    Dereference(Dereference),
    Disambiguation(Disambiguation),
    FieldAccess(FieldAccess),
    ForLoop(ForLoop),
    If(If),
    IfLet(IfLet),
    Let(Let),
    Loop(Loop),
    MacroCall(MacroCall),
    Match(Match),
    Number(Number),
    Parenthetical(Parenthetical),
    Range(Range),
    Reference(Reference),
    Return(Return),
    Slice(Slice),
    String(String),
    Tuple(Tuple),
    TryOperator(TryOperator),
    Unary(Unary),
    UnsafeBlock(UnsafeBlock),
    Value(Value),
    While(While),
    WhileLet(WhileLet),
}

impl Expression {
    pub fn extent(&self) -> Extent {
        match *self {
            Expression::Block(ref x) => x.extent,

            Expression::Array(ref x) => x.extent(),
            Expression::Number(ref x) => x.extent(),

            Expression::AsType(AsType { extent, .. })                 |
            Expression::Binary(Binary { extent, .. })                 |
            Expression::Box(ExpressionBox { extent, .. })             |
            Expression::Break(Break { extent, .. })                   |
            Expression::Byte(Byte { extent, .. })                     |
            Expression::ByteString(ByteString { extent, .. })         |
            Expression::Call(Call { extent, .. })                     |
            Expression::Character(Character { extent, .. })           |
            Expression::Closure(Closure { extent, .. })               |
            Expression::Continue(Continue { extent, .. })             |
            Expression::Dereference(Dereference { extent, .. })       |
            Expression::Disambiguation(Disambiguation { extent, .. }) |
            Expression::FieldAccess(FieldAccess { extent, .. })       |
            Expression::ForLoop(ForLoop { extent, .. })               |
            Expression::If(If { extent, .. })                         |
            Expression::IfLet(IfLet { extent, .. })                   |
            Expression::Let(Let { extent, .. })                       |
            Expression::Loop(Loop { extent, .. })                     |
            Expression::MacroCall(MacroCall { extent, .. })           |
            Expression::Match(Match { extent, .. })                   |
            Expression::Parenthetical(Parenthetical { extent, .. })   |
            Expression::Range(Range { extent, .. })                   |
            Expression::Reference(Reference { extent, .. })           |
            Expression::Return(Return { extent, .. })                 |
            Expression::Slice(Slice { extent, .. })                   |
            Expression::String(String { extent, .. })                 |
            Expression::TryOperator(TryOperator { extent, .. })       |
            Expression::Tuple(Tuple { extent, .. })                   |
            Expression::Unary(Unary { extent, .. })                   |
            Expression::UnsafeBlock(UnsafeBlock { extent, .. })       |
            Expression::Value(Value { extent, .. })                   |
            Expression::While(While { extent, .. })                   |
            Expression::WhileLet(WhileLet { extent, .. })             => extent,
        }
    }
}

#[derive(Debug, Visit)]
pub struct MacroCall {
    extent: Extent,
    name: Ident,
    arg: Option<Ident>,
    args: MacroCallArgs,
}

#[derive(Debug, Visit, Decompose)]
pub enum MacroCallArgs {
    Paren(Extent),
    Curly(Extent),
    Square(Extent),
}

#[derive(Debug, Visit)]
pub struct Let {
    extent: Extent,
    pattern: Pattern,
    typ: Option<Type>,
    value: Option<Box<Expression>>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct Tuple {
    extent: Extent,
    members: Vec<Expression>,
}

#[derive(Debug, Visit)]
pub struct TryOperator {
    extent: Extent,
    target: Box<Expression>,
}

#[derive(Debug, Visit)]
pub struct FieldAccess {
    extent: Extent,
    value: Box<Expression>,
    field: FieldName,
}

#[derive(Debug, Decompose)]
pub enum FieldName {
    Path(PathComponent),
    Number(Extent),
}

#[derive(Debug, Visit, Decompose)]
pub enum Number {
    Binary(NumberBinary),
    Decimal(NumberDecimal),
    Hexadecimal(NumberHexadecimal),
    Octal(NumberOctal),
}

impl Number {
    fn extent(&self) -> Extent {
        match *self {
            Number::Binary(NumberBinary { extent, .. })           |
            Number::Decimal(NumberDecimal { extent, .. })         |
            Number::Hexadecimal(NumberHexadecimal { extent, .. }) |
            Number::Octal(NumberOctal { extent, .. })             => extent,
        }
    }
}

#[derive(Debug)]
pub enum NumberSuffix {
    F32,
    F64,
    U8,
    U16,
    U32,
    U64,
    Usize,
    I8,
    I16,
    I32,
    I64,
    Isize,
}

#[derive(Debug, Visit)]
pub struct NumberBinary {
    extent: Extent,
    decimal: Extent,
    fraction: Option<Extent>,
    exponent: Option<Extent>,
    suffix: Option<NumberSuffix>,
}

#[derive(Debug, Visit)]
pub struct NumberDecimal {
    extent: Extent,
    decimal: Extent,
    fraction: Option<Extent>,
    exponent: Option<Extent>,
    suffix: Option<NumberSuffix>,
}

#[derive(Debug, Visit)]
pub struct NumberHexadecimal {
    extent: Extent,
    decimal: Extent,
    fraction: Option<Extent>,
    exponent: Option<Extent>,
    suffix: Option<NumberSuffix>,
}

#[derive(Debug, Visit)]
pub struct NumberOctal {
    extent: Extent,
    decimal: Extent,
    fraction: Option<Extent>,
    exponent: Option<Extent>,
    suffix: Option<NumberSuffix>,
}

#[derive(Debug, Visit)]
pub struct Value {
    extent: Extent,
    name: PathedIdent,
    literal: Option<StructLiteral>,
}

#[derive(Debug, Visit)]
pub struct StructLiteral {
    extent: Extent,
    fields: Vec<StructLiteralField>,
    splat: Option<Box<Expression>>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct StructLiteralField {
    name: Ident,
    value: Expression,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct Call {
    extent: Extent,
    target: Box<Expression>,
    args: Vec<Expression>,
}

#[derive(Debug, Visit)]
pub struct ForLoop {
    extent: Extent,
    label: Option<Lifetime>,
    pattern: Pattern,
    iter: Box<Expression>,
    body: Box<Block>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct Loop {
    extent: Extent,
    label: Option<Lifetime>,
    body: Box<Block>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct IfLet {
    extent: Extent,
    pattern: Pattern,
    value: Box<Expression>,
    body: Box<Block>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct While {
    extent: Extent,
    label: Option<Lifetime>,
    value: Box<Expression>,
    body: Box<Block>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct WhileLet {
    extent: Extent,
    label: Option<Lifetime>,
    pattern: Pattern,
    value: Box<Expression>,
    body: Box<Block>,
    whitespace: Vec<Whitespace>,
}

// TODO: Should this be the same as dereference? What about reference?
#[derive(Debug, Visit)]
pub struct Unary {
    extent: Extent,
    op: UnaryOp,
    value: Box<Expression>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug)]
pub enum UnaryOp {
    Negate,
    Not,
}

#[derive(Debug, Visit)]
pub struct Binary {
    extent: Extent,
    op: BinaryOp,
    lhs: Box<Expression>,
    rhs: Box<Expression>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug)]
pub enum BinaryOp {
    Add,
    AddAssign,
    Assign,
    BitwiseAnd,
    BitwiseAndAssign,
    BitwiseOr,
    BitwiseOrAssign,
    BitwiseXor,
    BitwiseXorAssign,
    BooleanAnd,
    BooleanOr,
    Div,
    DivAssign,
    Equal,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    Mod,
    ModAssign,
    Mul,
    MulAssign,
    NotEqual,
    ShiftLeft,
    ShiftLeftAssign,
    ShiftRight,
    ShiftRightAssign,
    Sub,
    SubAssign,
}

#[derive(Debug, Visit)]
pub struct If {
    extent: Extent,
    condition: Box<Expression>,
    body: Box<Block>,
    more: Vec<If>,
    else_body: Option<Box<Block>>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct Match {
    extent: Extent,
    head: Box<Expression>,
    arms: Vec<MatchArm>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct MatchArm {
    extent: Extent,
    attributes: Vec<Attribute>,
    pattern: Vec<Pattern>,
    guard: Option<Expression>,
    hand: MatchHand,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit, Decompose)]
pub enum MatchHand {
    Brace(Expression),
    Expression(Expression),
}

#[derive(Debug, Visit)]
pub struct Range {
    extent: Extent,
    lhs: Option<Box<Expression>>,
    rhs: Option<Box<Expression>>,
}

#[derive(Debug, Visit, Decompose)]
pub enum Array {
    Explicit(ArrayExplicit),
    Repeated(ArrayRepeated),
}

impl Array {
    fn extent(&self) -> Extent {
        match *self {
            Array::Explicit(ArrayExplicit { extent, .. }) |
            Array::Repeated(ArrayRepeated { extent, .. }) => extent,
        }
    }
}

#[derive(Debug, Visit)]
pub struct ArrayExplicit {
    extent: Extent,
    values: Vec<Expression>,
}

#[derive(Debug, Visit)]
pub struct ArrayRepeated {
    extent: Extent,
    value: Box<Expression>,
    count: Box<Expression>,
    whitespace: Vec<Whitespace>,
}

// TODO: Rename this visitor function?
#[derive(Debug, Visit)]
pub struct ExpressionBox {
    extent: Extent,
    value: Box<Expression>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct AsType {
    extent: Extent,
    value: Box<Expression>,
    typ: Type,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct Character {
    extent: Extent,
    value: Extent,
}

#[derive(Debug, Visit)]
pub struct String {
    extent: Extent,
    value: Extent,
}

#[derive(Debug, Visit)]
pub struct Byte {
    extent: Extent,
    value: Character,
}

#[derive(Debug, Visit)]
pub struct ByteString {
    extent: Extent,
    value: String,
}

#[derive(Debug, Visit)]
pub struct Slice {
    extent: Extent,
    target: Box<Expression>,
    range: Box<Expression>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct Closure {
    extent: Extent,
    #[visit(ignore)]
    is_move: bool,
    args: Vec<ClosureArg>,
    return_type: Option<Type>,
    body: Box<Expression>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct ClosureArg {
    name: Pattern,
    typ: Option<Type>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct Reference {
    extent: Extent,
    mutable: Option<Extent>,
    value: Box<Expression>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct Dereference {
    extent: Extent,
    value: Box<Expression>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct Disambiguation {
    extent: Extent,
    from_type: Type,
    to_type: Option<TypeNamed>,
    components: Vec<PathComponent>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct Return {
    extent: Extent,
    value: Option<Box<Expression>>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct Continue {
    extent: Extent,
    label: Option<Lifetime>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct Break {
    extent: Extent,
    label: Option<Lifetime>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug)]
enum ExpressionTail {
    AsType { typ: Type, whitespace: Vec<Whitespace> },
    Binary { op: BinaryOp, rhs: Box<Expression>, whitespace: Vec<Whitespace> },
    FieldAccess { field: FieldName },
    Call { args: Vec<Expression> },
    Range { rhs: Option<Box<Expression>> },
    Slice { range: Box<Expression>, whitespace: Vec<Whitespace> },
    TryOperator,
}

#[derive(Debug, Visit)]
pub struct Pattern {
    extent: Extent,
    name: Option<PatternName>,
    kind: PatternKind,
}

#[derive(Debug, Visit)]
pub struct PatternName {
    extent: Extent,
    is_ref: Option<Extent>,
    is_mut: Option<Extent>,
    name: Ident,
    whitespace: Vec<Whitespace>,
}

impl Pattern {
    #[allow(dead_code)]
    fn extent(&self) -> Extent {
        self.extent
    }
}

#[derive(Debug, Visit, Decompose)]
pub enum PatternKind {
    Byte(PatternByte),
    ByteString(PatternByteString),
    Character(PatternCharacter),
    Ident(PatternIdent), // TODO: split into ident and enumtuple
    Number(PatternNumber),
    Range(PatternRange),
    Reference(PatternReference),
    String(PatternString),
    Struct(PatternStruct),
    Tuple(PatternTuple),
}

impl PatternKind {
    #[allow(dead_code)]
    fn extent(&self) -> Extent {
        use PatternKind::*;

        match *self {
            Byte(PatternByte { extent, .. })             |
            ByteString(PatternByteString { extent, .. }) |
            Character(PatternCharacter { extent, .. })   |
            Ident(PatternIdent { extent, .. })           |
            Number(PatternNumber { extent, .. })         |
            Range(PatternRange { extent, .. })           |
            Reference(PatternReference { extent, .. })   |
            String(PatternString { extent, .. })         |
            Struct(PatternStruct { extent, .. })         |
            Tuple(PatternTuple { extent, .. })           => extent,
        }
    }
}

#[derive(Debug, Visit)]
pub struct PatternIdent {
    extent: Extent,
    is_ref: Option<Extent>,
    is_mut: Option<Extent>,
    ident: PathedIdent,
    tuple: Option<PatternTuple>,
}

#[derive(Debug, Visit)]
pub struct PatternStruct {
    extent: Extent,
    name: PathedIdent,
    fields: Vec<PatternStructField>,
    #[visit(ignore)]
    wildcard: bool,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit, Decompose)]
pub enum PatternStructField {
    Long(PatternStructFieldLong),
    Short(PatternStructFieldShort),
}

#[derive(Debug, Visit)]
pub struct PatternStructFieldLong {
    extent: Extent,
    name: Ident,
    pattern: Pattern,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct PatternStructFieldShort {
    ident: PatternIdent
}

#[derive(Debug, Visit)]
pub struct PatternTuple {
    extent: Extent,
    members: Vec<PatternTupleMember>,
}

#[derive(Debug, Visit, Decompose)]
pub enum PatternTupleMember {
    Pattern(Pattern),
    Wildcard(Extent),
}

#[derive(Debug, Visit)]
pub struct PatternWildcard {
    extent: Extent,
}

#[derive(Debug, Visit)]
pub struct PatternByte {
    extent: Extent,
    value: Byte,
}

#[derive(Debug, Visit)]
pub struct PatternCharacter {
    extent: Extent,
    value: Character,
}

#[derive(Debug, Visit)]
pub struct PatternByteString {
    extent: Extent,
    value: ByteString,
}

#[derive(Debug, Visit)]
pub struct PatternString {
    extent: Extent,
    value: String,
}

#[derive(Debug, Visit)]
pub struct PatternNumber {
    extent: Extent,
    value: Number,
}

#[derive(Debug, Visit)]
pub struct PatternRange {
    extent: Extent,
    start: PatternRangeComponent,
    end: PatternRangeComponent,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Decompose)]
pub enum PatternRangeComponent {
    Byte(Byte),
    Character(Character),
    Number(Number),
}

#[derive(Debug, Visit)]
pub struct PatternReference {
    extent: Extent,
    is_mut: Option<Extent>,
    pattern: Box<Pattern>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct Trait {
    extent: Extent,
    visibility: Option<Visibility>,
    is_unsafe: Option<Extent>,
    name: Ident,
    generics: Option<GenericDeclarations>,
    bounds: Option<TraitBounds>,
    wheres: Vec<Where>,
    members: Vec<TraitMember>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit, Decompose)]
pub enum TraitMember {
    Attribute(Attribute),
    Function(TraitMemberFunction),
    Type(TraitMemberType),
    Whitespace(Vec<Whitespace>),
}

#[derive(Debug, Visit)]
pub struct TraitMemberFunction {
    extent: Extent,
    header: TraitImplFunctionHeader,
    body: Option<Block>,
}

#[derive(Debug, Visit)]
pub struct TraitMemberType {
    extent: Extent,
    name: Ident,
    bounds: Option<TraitBounds>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct Impl {
    extent: Extent,
    is_unsafe: Option<Extent>,
    generics: Option<GenericDeclarations>,
    of_trait: Option<ImplOfTrait>,
    type_name: Type,
    wheres: Vec<Where>,
    body: Vec<ImplMember>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct ImplOfTrait {
    extent: Extent,
    is_negative: Option<Extent>,
    trait_name: Type, // TODO: namedtype only?
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit, Decompose)]
pub enum ImplMember {
    Attribute(Attribute),
    Function(ImplFunction),
    Type(ImplType),
    MacroCall(MacroCall),
    Whitespace(Vec<Whitespace>),
}

#[derive(Debug, Visit)]
pub struct ImplFunction {
    extent: Extent,
    header: FunctionHeader,
    body: Block,
}

#[derive(Debug, Visit)]
pub struct ImplType {
    extent: Extent,
    name: Ident,
    typ: Type,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct Crate {
    extent: Extent,
    visibility: Option<Visibility>,
    name: Ident,
    rename: Option<Ident>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct ExternBlock {
    extent: Extent,
    abi: Option<String>,
    members: Vec<ExternBlockMember>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit, Decompose)]
pub enum ExternBlockMember {
    Attribute(Attribute),
    Function(ExternBlockMemberFunction),
    Whitespace(Vec<Whitespace>),
}

#[derive(Debug, Visit)]
pub struct ExternBlockMemberFunction {
    extent: Extent,
    visibility: Option<Visibility>,
    pub name: Ident,
    arguments: Vec<ExternBlockMemberFunctionArgument>,
    return_type: Option<Type>,
    wheres: Vec<Where>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit, Decompose)]
pub enum ExternBlockMemberFunctionArgument {
    Named(ExternBlockMemberFunctionArgumentNamed),
    Variadic(ExternBlockMemberFunctionArgumentVariadic),
}

#[derive(Debug, Visit)]
pub struct ExternBlockMemberFunctionArgumentNamed {
    extent: Extent,
    name: Pattern,
    typ: Type,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct ExternBlockMemberFunctionArgumentVariadic {
    extent: Extent,
}

#[derive(Debug, Visit)]
pub struct TypeAlias {
    extent: Extent,
    visibility: Option<Visibility>,
    name: Type,
    defn: Type,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct Module {
    extent: Extent,
    visibility: Option<Visibility>,
    name: Ident,
    body: Option<Vec<Item>>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug, Visit)]
pub struct Visibility {
    extent: Extent,
    #[visit(ignore)]
    qualifier: Option<VisibilityQualifier>,
    whitespace: Vec<Whitespace>,
}

#[derive(Debug)]
pub enum VisibilityQualifier {
    Crate,
    Path(Path),
}

// --------------------------------------------------

pub trait Visit {
    fn visit<V>(&self, &mut V)
        where V: Visitor;
}

impl<T> Visit for Box<T>
    where T: Visit
{
    fn visit<V>(&self, v: &mut V)
        where V: Visitor
    {
        (**self).visit(v)
    }
}

impl<T> Visit for Option<T>
    where T: Visit
{
    fn visit<V>(&self, v: &mut V)
        where V: Visitor
    {
        for i in self {
            i.visit(v)
        }
    }
}

impl<T> Visit for Vec<T>
    where T: Visit
{
    fn visit<V>(&self, v: &mut V)
        where V: Visitor
    {
        for i in self {
            i.visit(v)
        }
    }
}

// Cheap hacks to avoid having to annotate every terminal `Extent` and
// enum; just visit them and don't do anything.

// An extent without any context is pretty useless.
impl Visit for Extent {
    fn visit<V>(&self, _v: &mut V)
        where V: Visitor
    {}
}

// Can't imagine we'd ever want to count the number of additions;
// without the lhs/rhs there's not much benefit.
impl Visit for UnaryOp {
    fn visit<V>(&self, _v: &mut V)
        where V: Visitor
    {}
}
impl Visit for BinaryOp {
    fn visit<V>(&self, _v: &mut V)
        where V: Visitor
    {}
}

// We *might* want to visit this, to enable checking for "large" tuple
// indexes or poor variable names?
impl Visit for FieldName {
    fn visit<V>(&self, _v: &mut V)
        where V: Visitor
    {}
}

// We *might* want to continue visiting the children to be able to
// inspect the character / number?
impl Visit for PatternRangeComponent {
    fn visit<V>(&self, _v: &mut V)
        where V: Visitor
    {}
}

// Knowing if an unknown pointer is mutable has no benefit.
impl Visit for TypePointerKind {
    fn visit<V>(&self, _v: &mut V)
        where V: Visitor
    {}
}

// Knowing if an unknown number literal is usize? Seems unlikely
impl Visit for NumberSuffix {
    fn visit<V>(&self, _v: &mut V)
        where V: Visitor
    {}
}

pub trait Visitor {
    fn visit_argument(&mut self, &Argument) {}
    fn visit_array(&mut self, &Array) {}
    fn visit_array_explicit(&mut self, &ArrayExplicit) {}
    fn visit_array_repeated(&mut self, &ArrayRepeated) {}
    fn visit_as_type(&mut self, &AsType) {}
    fn visit_associated_type(&mut self, &AssociatedType) {}
    fn visit_attribute(&mut self, &Attribute) {}
    fn visit_binary(&mut self, &Binary) {}
    fn visit_block(&mut self, &Block) {}
    fn visit_break(&mut self, &Break) {}
    fn visit_byte(&mut self, &Byte) {}
    fn visit_byte_string(&mut self, &ByteString) {}
    fn visit_call(&mut self, &Call) {}
    fn visit_character(&mut self, &Character) {}
    fn visit_closure(&mut self, &Closure) {}
    fn visit_closure_arg(&mut self, &ClosureArg) {}
    fn visit_comment(&mut self, &Comment) {}
    fn visit_const(&mut self, &Const) {}
    fn visit_continue(&mut self, &Continue) {}
    fn visit_crate(&mut self, &Crate) {}
    fn visit_dereference(&mut self, &Dereference) {}
    fn visit_disambiguation(&mut self, &Disambiguation) {}
    fn visit_enum(&mut self, &Enum) {}
    fn visit_enum_variant(&mut self, &EnumVariant) {}
    fn visit_enum_variant_body(&mut self, &EnumVariantBody) {}
    fn visit_expression(&mut self, &Expression) {}
    fn visit_expression_box(&mut self, &ExpressionBox) {}
    fn visit_extern_block(&mut self, &ExternBlock) {}
    fn visit_extern_block_member(&mut self, &ExternBlockMember) {}
    fn visit_extern_block_member_function(&mut self, &ExternBlockMemberFunction) {}
    fn visit_extern_block_member_function_argument(&mut self, &ExternBlockMemberFunctionArgument) {}
    fn visit_extern_block_member_function_argument_named(&mut self, &ExternBlockMemberFunctionArgumentNamed) {}
    fn visit_extern_block_member_function_argument_variadic(&mut self, &ExternBlockMemberFunctionArgumentVariadic) {}
    fn visit_field_access(&mut self, &FieldAccess) {}
    fn visit_file(&mut self, &File) {}
    fn visit_for_loop(&mut self, &ForLoop) {}
    fn visit_function(&mut self, &Function) {}
    fn visit_function_header(&mut self, &FunctionHeader) {}
    fn visit_function_qualifiers(&mut self, &FunctionQualifiers) {}
    fn visit_generic_declaration_lifetime(&mut self, &GenericDeclarationLifetime) {}
    fn visit_generic_declaration_type(&mut self, &GenericDeclarationType) {}
    fn visit_generic_declaration_type_addition(&mut self, &GenericDeclarationTypeAddition) {}
    fn visit_generic_declarations(&mut self, &GenericDeclarations) {}
    fn visit_ident(&mut self, &Ident) {}
    fn visit_if(&mut self, &If) {}
    fn visit_if_let(&mut self, &IfLet) {}
    fn visit_impl(&mut self, &Impl) {}
    fn visit_impl_function(&mut self, &ImplFunction) {}
    fn visit_impl_member(&mut self, &ImplMember) {}
    fn visit_impl_of_trait(&mut self, &ImplOfTrait) {}
    fn visit_impl_type(&mut self, &ImplType) {}
    fn visit_item(&mut self, &Item) {}
    fn visit_let(&mut self, &Let) {}
    fn visit_lifetime(&mut self, &Lifetime) {}
    fn visit_loop(&mut self, &Loop) {}
    fn visit_macro_call(&mut self, &MacroCall) {}
    fn visit_macro_call_args(&mut self, &MacroCallArgs) {}
    fn visit_match(&mut self, &Match) {}
    fn visit_match_arm(&mut self, &MatchArm) {}
    fn visit_match_hand(&mut self, &MatchHand) {}
    fn visit_module(&mut self, &Module) {}
    fn visit_named_argument(&mut self, &NamedArgument) {}
    fn visit_number(&mut self, &Number) {}
    fn visit_number_binary(&mut self, &NumberBinary) {}
    fn visit_number_decimal(&mut self, &NumberDecimal) {}
    fn visit_number_hexadecimal(&mut self, &NumberHexadecimal) {}
    fn visit_number_octal(&mut self, &NumberOctal) {}
    fn visit_parenthetical(&mut self, &Parenthetical) {}
    fn visit_path(&mut self, &Path) {}
    fn visit_path_component(&mut self, &PathComponent) {}
    fn visit_pathed_ident(&mut self, &PathedIdent) {}
    fn visit_pattern(&mut self, &Pattern) {}
    fn visit_pattern_name(&mut self, &PatternName) {}
    fn visit_pattern_kind(&mut self, &PatternKind) {}
    fn visit_pattern_byte(&mut self, &PatternByte) {}
    fn visit_pattern_byte_string(&mut self, &PatternByteString) {}
    fn visit_pattern_character(&mut self, &PatternCharacter) {}
    fn visit_pattern_ident(&mut self, &PatternIdent) {}
    fn visit_pattern_number(&mut self, &PatternNumber) {}
    fn visit_pattern_range(&mut self, &PatternRange) {}
    fn visit_pattern_reference(&mut self, &PatternReference) {}
    fn visit_pattern_string(&mut self, &PatternString) {}
    fn visit_pattern_struct(&mut self, &PatternStruct) {}
    fn visit_pattern_struct_field(&mut self, &PatternStructField) {}
    fn visit_pattern_struct_field_long(&mut self, &PatternStructFieldLong) {}
    fn visit_pattern_struct_field_short(&mut self, &PatternStructFieldShort) {}
    fn visit_pattern_tuple(&mut self, &PatternTuple) {}
    fn visit_pattern_tuple_member(&mut self, &PatternTupleMember) {}
    fn visit_pattern_wildcard(&mut self, &PatternWildcard) {}
    fn visit_range(&mut self, &Range) {}
    fn visit_reference(&mut self, &Reference) {}
    fn visit_return(&mut self, &Return) {}
    fn visit_self_argument(&mut self, &SelfArgument) {}
    fn visit_self_argument_longhand(&mut self, &SelfArgumentLonghand) {}
    fn visit_self_argument_shorthand(&mut self, &SelfArgumentShorthand) {}
    fn visit_self_argument_shorthand_qualifier(&mut self, &SelfArgumentShorthandQualifier) {}
    fn visit_slice(&mut self, &Slice) {}
    fn visit_statement(&mut self, &Statement) {}
    fn visit_static(&mut self, &Static) {}
    fn visit_string(&mut self, &String) {}
    fn visit_struct(&mut self, &Struct) {}
    fn visit_struct_definition_body(&mut self, &StructDefinitionBody) {}
    fn visit_struct_definition_body_brace(&mut self, &StructDefinitionBodyBrace) {}
    fn visit_struct_definition_body_tuple(&mut self, &StructDefinitionBodyTuple) {}
    fn visit_struct_definition_field_named(&mut self, &StructDefinitionFieldNamed) {}
    fn visit_struct_definition_field_unnamed(&mut self, &StructDefinitionFieldUnnamed) {}
    fn visit_struct_literal(&mut self, &StructLiteral) {}
    fn visit_struct_literal_field(&mut self, &StructLiteralField) {}
    fn visit_trait(&mut self, &Trait) {}
    fn visit_trait_bound(&mut self, &TraitBound) {}
    fn visit_trait_bound_lifetime(&mut self, &TraitBoundLifetime) {}
    fn visit_trait_bound_normal(&mut self, &TraitBoundNormal) {}
    fn visit_trait_bound_relaxed(&mut self, &TraitBoundRelaxed) {}
    fn visit_trait_bound_type(&mut self, &TraitBoundType) {}
    fn visit_trait_bounds(&mut self, &TraitBounds) {}
    fn visit_trait_impl_argument(&mut self, &TraitImplArgument) {}
    fn visit_trait_impl_argument_named(&mut self, &TraitImplArgumentNamed) {}
    fn visit_trait_impl_function_header(&mut self, &TraitImplFunctionHeader) {}
    fn visit_trait_member(&mut self, &TraitMember) {}
    fn visit_trait_member_function(&mut self, &TraitMemberFunction) {}
    fn visit_trait_member_type(&mut self, &TraitMemberType) {}
    fn visit_try_operator(&mut self, &TryOperator) {}
    fn visit_tuple(&mut self, &Tuple) {}
    fn visit_turbofish(&mut self, &Turbofish) {}
    fn visit_type(&mut self, &Type) {}
    fn visit_type_alias(&mut self, &TypeAlias) {}
    fn visit_type_array(&mut self, &TypeArray) {}
    fn visit_type_combination(&mut self, &TypeCombination) {}
    fn visit_type_combination_additional(&mut self, &TypeCombinationAdditional) {}
    fn visit_type_combination_base(&mut self, &TypeCombinationBase) {}
    fn visit_type_disambiguation(&mut self, &TypeDisambiguation) {}
    fn visit_type_function(&mut self, &TypeFunction) {}
    fn visit_type_generics(&mut self, &TypeGenerics) {}
    fn visit_type_generics_angle(&mut self, &TypeGenericsAngle) {}
    fn visit_type_generics_angle_member(&mut self, &TypeGenericsAngleMember) {}
    fn visit_type_generics_function(&mut self, &TypeGenericsFunction) {}
    fn visit_type_higher_ranked_trait_bounds(&mut self, &TypeHigherRankedTraitBounds) {}
    fn visit_type_higher_ranked_trait_bounds_child(&mut self, &TypeHigherRankedTraitBoundsChild) {}
    fn visit_type_impl_trait(&mut self, &TypeImplTrait) {}
    fn visit_type_named(&mut self, &TypeNamed) {}
    fn visit_type_named_component(&mut self, &TypeNamedComponent) {}
    fn visit_type_pointer(&mut self, &TypePointer) {}
    fn visit_type_reference(&mut self, &TypeReference) {}
    fn visit_type_reference_kind(&mut self, &TypeReferenceKind) {}
    fn visit_type_slice(&mut self, &TypeSlice) {}
    fn visit_type_tuple(&mut self, &TypeTuple) {}
    fn visit_unary(&mut self, &Unary) {}
    fn visit_unsafe_block(&mut self, &UnsafeBlock) {}
    fn visit_use(&mut self, &Use) {}
    fn visit_use_tail(&mut self, &UseTail) {}
    fn visit_use_tail_glob(&mut self, &UseTailGlob) {}
    fn visit_use_tail_ident(&mut self, &UseTailIdent) {}
    fn visit_use_tail_multi(&mut self, &UseTailMulti) {}
    fn visit_value(&mut self, &Value) {}
    fn visit_visibility(&mut self, &Visibility) {}
    fn visit_where(&mut self, &Where) {}
    fn visit_where_lifetime(&mut self, &WhereLifetime) {}
    fn visit_where_type(&mut self, &WhereType) {}
    fn visit_while(&mut self, &While) {}
    fn visit_while_let(&mut self, &WhileLet) {}
    fn visit_whitespace(&mut self, &Whitespace) {}

    fn exit_argument(&mut self, &Argument) {}
    fn exit_array(&mut self, &Array) {}
    fn exit_array_explicit(&mut self, &ArrayExplicit) {}
    fn exit_array_repeated(&mut self, &ArrayRepeated) {}
    fn exit_as_type(&mut self, &AsType) {}
    fn exit_associated_type(&mut self, &AssociatedType) {}
    fn exit_attribute(&mut self, &Attribute) {}
    fn exit_binary(&mut self, &Binary) {}
    fn exit_block(&mut self, &Block) {}
    fn exit_break(&mut self, &Break) {}
    fn exit_byte(&mut self, &Byte) {}
    fn exit_byte_string(&mut self, &ByteString) {}
    fn exit_call(&mut self, &Call) {}
    fn exit_character(&mut self, &Character) {}
    fn exit_closure(&mut self, &Closure) {}
    fn exit_closure_arg(&mut self, &ClosureArg) {}
    fn exit_comment(&mut self, &Comment) {}
    fn exit_const(&mut self, &Const) {}
    fn exit_continue(&mut self, &Continue) {}
    fn exit_crate(&mut self, &Crate) {}
    fn exit_dereference(&mut self, &Dereference) {}
    fn exit_disambiguation(&mut self, &Disambiguation) {}
    fn exit_enum(&mut self, &Enum) {}
    fn exit_enum_variant(&mut self, &EnumVariant) {}
    fn exit_enum_variant_body(&mut self, &EnumVariantBody) {}
    fn exit_expression(&mut self, &Expression) {}
    fn exit_expression_box(&mut self, &ExpressionBox) {}
    fn exit_extern_block(&mut self, &ExternBlock) {}
    fn exit_extern_block_member(&mut self, &ExternBlockMember) {}
    fn exit_extern_block_member_function(&mut self, &ExternBlockMemberFunction) {}
    fn exit_extern_block_member_function_argument(&mut self, &ExternBlockMemberFunctionArgument) {}
    fn exit_extern_block_member_function_argument_named(&mut self, &ExternBlockMemberFunctionArgumentNamed) {}
    fn exit_extern_block_member_function_argument_variadic(&mut self, &ExternBlockMemberFunctionArgumentVariadic) {}
    fn exit_field_access(&mut self, &FieldAccess) {}
    fn exit_file(&mut self, &File) {}
    fn exit_for_loop(&mut self, &ForLoop) {}
    fn exit_function(&mut self, &Function) {}
    fn exit_function_header(&mut self, &FunctionHeader) {}
    fn exit_function_qualifiers(&mut self, &FunctionQualifiers) {}
    fn exit_generic_declaration_lifetime(&mut self, &GenericDeclarationLifetime) {}
    fn exit_generic_declaration_type(&mut self, &GenericDeclarationType) {}
    fn exit_generic_declaration_type_addition(&mut self, &GenericDeclarationTypeAddition) {}
    fn exit_generic_declarations(&mut self, &GenericDeclarations) {}
    fn exit_ident(&mut self, &Ident) {}
    fn exit_if(&mut self, &If) {}
    fn exit_if_let(&mut self, &IfLet) {}
    fn exit_impl(&mut self, &Impl) {}
    fn exit_impl_function(&mut self, &ImplFunction) {}
    fn exit_impl_member(&mut self, &ImplMember) {}
    fn exit_impl_of_trait(&mut self, &ImplOfTrait) {}
    fn exit_impl_type(&mut self, &ImplType) {}
    fn exit_item(&mut self, &Item) {}
    fn exit_let(&mut self, &Let) {}
    fn exit_lifetime(&mut self, &Lifetime) {}
    fn exit_loop(&mut self, &Loop) {}
    fn exit_macro_call(&mut self, &MacroCall) {}
    fn exit_macro_call_args(&mut self, &MacroCallArgs) {}
    fn exit_match(&mut self, &Match) {}
    fn exit_match_arm(&mut self, &MatchArm) {}
    fn exit_match_hand(&mut self, &MatchHand) {}
    fn exit_module(&mut self, &Module) {}
    fn exit_named_argument(&mut self, &NamedArgument) {}
    fn exit_number(&mut self, &Number) {}
    fn exit_number_binary(&mut self, &NumberBinary) {}
    fn exit_number_decimal(&mut self, &NumberDecimal) {}
    fn exit_number_hexadecimal(&mut self, &NumberHexadecimal) {}
    fn exit_number_octal(&mut self, &NumberOctal) {}
    fn exit_parenthetical(&mut self, &Parenthetical) {}
    fn exit_path(&mut self, &Path) {}
    fn exit_path_component(&mut self, &PathComponent) {}
    fn exit_pathed_ident(&mut self, &PathedIdent) {}
    fn exit_pattern(&mut self, &Pattern) {}
    fn exit_pattern_name(&mut self, &PatternName) {}
    fn exit_pattern_kind(&mut self, &PatternKind) {}
    fn exit_pattern_byte(&mut self, &PatternByte) {}
    fn exit_pattern_byte_string(&mut self, &PatternByteString) {}
    fn exit_pattern_character(&mut self, &PatternCharacter) {}
    fn exit_pattern_ident(&mut self, &PatternIdent) {}
    fn exit_pattern_number(&mut self, &PatternNumber) {}
    fn exit_pattern_range(&mut self, &PatternRange) {}
    fn exit_pattern_reference(&mut self, &PatternReference) {}
    fn exit_pattern_string(&mut self, &PatternString) {}
    fn exit_pattern_struct(&mut self, &PatternStruct) {}
    fn exit_pattern_struct_field(&mut self, &PatternStructField) {}
    fn exit_pattern_struct_field_long(&mut self, &PatternStructFieldLong) {}
    fn exit_pattern_struct_field_short(&mut self, &PatternStructFieldShort) {}
    fn exit_pattern_tuple(&mut self, &PatternTuple) {}
    fn exit_pattern_tuple_member(&mut self, &PatternTupleMember) {}
    fn exit_pattern_wildcard(&mut self, &PatternWildcard) {}
    fn exit_range(&mut self, &Range) {}
    fn exit_reference(&mut self, &Reference) {}
    fn exit_return(&mut self, &Return) {}
    fn exit_self_argument(&mut self, &SelfArgument) {}
    fn exit_self_argument_longhand(&mut self, &SelfArgumentLonghand) {}
    fn exit_self_argument_shorthand(&mut self, &SelfArgumentShorthand) {}
    fn exit_self_argument_shorthand_qualifier(&mut self, &SelfArgumentShorthandQualifier) {}
    fn exit_slice(&mut self, &Slice) {}
    fn exit_statement(&mut self, &Statement) {}
    fn exit_static(&mut self, &Static) {}
    fn exit_string(&mut self, &String) {}
    fn exit_struct(&mut self, &Struct) {}
    fn exit_struct_definition_body(&mut self, &StructDefinitionBody) {}
    fn exit_struct_definition_body_brace(&mut self, &StructDefinitionBodyBrace) {}
    fn exit_struct_definition_body_tuple(&mut self, &StructDefinitionBodyTuple) {}
    fn exit_struct_definition_field_named(&mut self, &StructDefinitionFieldNamed) {}
    fn exit_struct_definition_field_unnamed(&mut self, &StructDefinitionFieldUnnamed) {}
    fn exit_struct_literal(&mut self, &StructLiteral) {}
    fn exit_struct_literal_field(&mut self, &StructLiteralField) {}
    fn exit_trait(&mut self, &Trait) {}
    fn exit_trait_bound(&mut self, &TraitBound) {}
    fn exit_trait_bound_lifetime(&mut self, &TraitBoundLifetime) {}
    fn exit_trait_bound_normal(&mut self, &TraitBoundNormal) {}
    fn exit_trait_bound_relaxed(&mut self, &TraitBoundRelaxed) {}
    fn exit_trait_bound_type(&mut self, &TraitBoundType) {}
    fn exit_trait_bounds(&mut self, &TraitBounds) {}
    fn exit_trait_impl_argument(&mut self, &TraitImplArgument) {}
    fn exit_trait_impl_argument_named(&mut self, &TraitImplArgumentNamed) {}
    fn exit_trait_impl_function_header(&mut self, &TraitImplFunctionHeader) {}
    fn exit_trait_member(&mut self, &TraitMember) {}
    fn exit_trait_member_function(&mut self, &TraitMemberFunction) {}
    fn exit_trait_member_type(&mut self, &TraitMemberType) {}
    fn exit_try_operator(&mut self, &TryOperator) {}
    fn exit_tuple(&mut self, &Tuple) {}
    fn exit_turbofish(&mut self, &Turbofish) {}
    fn exit_type(&mut self, &Type) {}
    fn exit_type_alias(&mut self, &TypeAlias) {}
    fn exit_type_array(&mut self, &TypeArray) {}
    fn exit_type_combination(&mut self, &TypeCombination) {}
    fn exit_type_combination_additional(&mut self, &TypeCombinationAdditional) {}
    fn exit_type_combination_base(&mut self, &TypeCombinationBase) {}
    fn exit_type_disambiguation(&mut self, &TypeDisambiguation) {}
    fn exit_type_function(&mut self, &TypeFunction) {}
    fn exit_type_generics(&mut self, &TypeGenerics) {}
    fn exit_type_generics_angle(&mut self, &TypeGenericsAngle) {}
    fn exit_type_generics_angle_member(&mut self, &TypeGenericsAngleMember) {}
    fn exit_type_generics_function(&mut self, &TypeGenericsFunction) {}
    fn exit_type_higher_ranked_trait_bounds(&mut self, &TypeHigherRankedTraitBounds) {}
    fn exit_type_higher_ranked_trait_bounds_child(&mut self, &TypeHigherRankedTraitBoundsChild) {}
    fn exit_type_impl_trait(&mut self, &TypeImplTrait) {}
    fn exit_type_named(&mut self, &TypeNamed) {}
    fn exit_type_named_component(&mut self, &TypeNamedComponent) {}
    fn exit_type_pointer(&mut self, &TypePointer) {}
    fn exit_type_reference(&mut self, &TypeReference) {}
    fn exit_type_reference_kind(&mut self, &TypeReferenceKind) {}
    fn exit_type_slice(&mut self, &TypeSlice) {}
    fn exit_type_tuple(&mut self, &TypeTuple) {}
    fn exit_unary(&mut self, &Unary) {}
    fn exit_unsafe_block(&mut self, &UnsafeBlock) {}
    fn exit_use(&mut self, &Use) {}
    fn exit_use_tail(&mut self, &UseTail) {}
    fn exit_use_tail_glob(&mut self, &UseTailGlob) {}
    fn exit_use_tail_ident(&mut self, &UseTailIdent) {}
    fn exit_use_tail_multi(&mut self, &UseTailMulti) {}
    fn exit_value(&mut self, &Value) {}
    fn exit_visibility(&mut self, &Visibility) {}
    fn exit_where(&mut self, &Where) {}
    fn exit_where_lifetime(&mut self, &WhereLifetime) {}
    fn exit_where_type(&mut self, &WhereType) {}
    fn exit_while(&mut self, &While) {}
    fn exit_while_let(&mut self, &WhileLet) {}
    fn exit_whitespace(&mut self, &Whitespace) {}
}

// --------------------------------------------------

fn ex(start: Point, end: Point) -> Extent {
    let ex = (start.offset, end.offset);
    assert!(ex.1 >= ex.0, "{} does not come before {}", ex.1, ex.0);
    ex
}

// --------------------------------------------------

fn ext<'s, F, T>(f: F) -> impl FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, Extent>
    where F: FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, T>
{
    move |pm, pt| {
        let spt = pt;
        let (pt, _) = try_parse!(f(pm, pt));
        Progress::success(pt, ex(spt, pt))
    }
}

fn parse_until<'s>(p: &'static str) -> impl Fn(&mut Master<'s>, Point<'s>) -> Progress<'s, Extent> {
    move |_, pt| {
        let spt = pt;
        let end = pt.s.find(p).unwrap_or(pt.s.len());
        let k = &pt.s[end..];
        let pt = Point { s: k, offset: pt.offset + end };

        Progress::success(pt, ex(spt, pt))
    }
}

fn parse_nested_tokens_until<'s, O, C>(is_open: O, is_close: C) ->
    impl Fn(&mut Master<'s>, Point<'s>) -> Progress<'s, Extent>
    where O: Fn(&Token<'s>) -> bool,
          C: Fn(&Token<'s>) -> bool,
{
    move |_, spt| {
        let mut bytes: usize = 0;
        let mut depth: usize = 0;

        for token in Tokens::new(spt.s) {
            if is_open(&token) {
                depth += 1;
            } else if is_close(&token) {
                if depth == 0 {
                    break;
                } else {
                    depth -= 1;
                }
            }

            bytes += token.data().len();
        }

        let pt = Point { s: &spt.s[bytes..], offset: spt.offset + bytes };

        Progress::success(pt, ex(spt, pt))
    }
}

fn optional_leading_whitespace<'s, F, T>(f: F) ->
    impl FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, T>
    where F: Fn(&mut Master<'s>, Point<'s>) -> Progress<'s, T>
{
    move |pm, pt| {
        sequence!(pm, pt, {
            _x    = optional_whitespace(Vec::new());
            value = rewind_on_error(pt, f);
        }, |_, _| value)
    }
}

// TODO: Maybe extract?
fn rewind_on_error<'s, F, T>(rewind_pt: Point<'s>, f: F) ->
    impl FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, T>
    where F: FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, T>
{
    move |pm, pt| {
        match f(pm, pt) {
            Progress { status: peresil::Status::Failure(f), .. } => {
                // TODO: unrecoverable errors
                Progress::failure(rewind_pt, f)
                // TODO: This failure does not occur at `pt`, what should we return instead?
            }
            other => other,
        }
    }
}

enum TailedState<P, T, E> {
    Nothing(P, E),
    ValueOnly(P, T),
    ValueAndSeparator(P, T),
}

fn parse_tailed<'s, F, T>(sep: &'static str, f: F, pm: &mut Master<'s>, pt: Point<'s>) ->
    TailedState<Point<'s>, T, Error>
    where F: Fn(&mut Master<'s>, Point<'s>) -> Progress<'s, T>
{
    match optional_leading_whitespace(f)(pm, pt) {
        Progress { status: peresil::Status::Failure(f), point } => {
            TailedState::Nothing(point, f)
        }
        Progress { status: peresil::Status::Success(value), point } => {
            match optional_leading_whitespace(literal(sep))(pm, point) {
                Progress { status: peresil::Status::Failure(_), point } => {
                    TailedState::ValueOnly(point, value)
                }
                Progress { status: peresil::Status::Success(_), point } => {
                    TailedState::ValueAndSeparator(point, value)
                }
            }
        }
    }
}

trait ImplicitSeparator {
    fn is_implicit_separator(&self) -> bool;
}

#[derive(Debug)]
struct Tailed<T> {
    values: Vec<T>,
    separator_count: usize,
    last_had_separator: bool,
}

impl<T> Default for Tailed<T> {
    fn default() -> Self {
        Tailed {
            values: Vec::new(),
            separator_count: 0,
            last_had_separator: false,
        }
    }
}

// Look for an expression that is followed by a separator. Each time
// the separator is found, another expression is attempted. Each
// expression is returned, along with the count of separators.
fn zero_or_more_tailed_append<'s, F, T>(append_to: Tailed<T>, sep: &'static str, f: F) ->
    impl FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, Tailed<T>>
    where F: Fn(&mut Master<'s>, Point<'s>) -> Progress<'s, T>
{
    move |pm, mut pt| {
        let mut tailed = append_to;
        loop {
            match parse_tailed(sep, &f, pm, pt) {
                TailedState::Nothing(pt, _) => {
                    return Progress::success(pt, tailed);
                }
                TailedState::ValueOnly(pt, v) => {
                    tailed.values.push(v);
                    tailed.last_had_separator = false;
                    return Progress::success(pt, tailed);
                }
                TailedState::ValueAndSeparator(pt2, v) => {
                    pt = pt2;
                    tailed.values.push(v);
                    tailed.separator_count += 1;
                    tailed.last_had_separator = true;
                }
            }
        }
    }
}

fn zero_or_more_tailed<'s, F, T>(sep: &'static str, f: F) ->
    impl FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, Tailed<T>>
    where F: Fn(&mut Master<'s>, Point<'s>) -> Progress<'s, T>
{
    zero_or_more_tailed_append(Tailed::default(), sep, f)
}

fn zero_or_more_tailed_values<'s, F, T>(sep: &'static str, f: F) ->
    impl FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, Vec<T>>
    where F: Fn(&mut Master<'s>, Point<'s>) -> Progress<'s, T>
{
    map(zero_or_more_tailed(sep, f), |t| t.values)
}

fn zero_or_more_tailed_values_append<'s, A, F, T>(append_to: A, sep: &'static str, f: F) ->
    impl FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, Vec<T>>
    where A: IntoAppend<T>,
          F: Fn(&mut Master<'s>, Point<'s>) -> Progress<'s, T>
{
    let append_to = append_to.into();
    // TODO: How do we reset separator_count and last_had_separator?
    let tailed = Tailed { values: append_to, ..Tailed::default() };
    map(zero_or_more_tailed_append(tailed, sep, f), |t| t.values)
}

// Used after parsing a single value, but not the separator
// Foo + Bar
//    ^
fn zero_or_more_tailed_values_resume<'s, F, T>(sep: &'static str, f: F) ->
    impl FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, Vec<T>>
    where F: Fn(&mut Master<'s>, Point<'s>) -> Progress<'s, T>
{
    move |pm, pt| {
        let spt = pt;
        let pt = match optional_leading_whitespace(literal(sep))(pm, pt) {
            Progress { status: peresil::Status::Failure(_), point } => {
                return Progress::success(point, Vec::new())
            }
            Progress { status: peresil::Status::Success(_), point } => {
                point
            }
        };

        match one_or_more_tailed_values(sep, f)(pm, pt) {
            Progress { status: peresil::Status::Failure(_), .. } => {
                // We parsed the separator, but not another value. Rewind to before the separator
                Progress::success(spt, Vec::new())
            }
            other => other
        }
    }
}

fn zero_or_more_implicitly_tailed_append<'s, F, T>(append_to: Tailed<T>, sep: &'static str, f: F) ->
    impl FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, Tailed<T>>
    where F: Fn(&mut Master<'s>, Point<'s>) -> Progress<'s, T>,
          T: ImplicitSeparator,
{
    move |pm, mut pt| {
        let mut tailed = append_to;
        loop {
            match parse_tailed(sep, &f, pm, pt) {
                TailedState::Nothing(pt, _) => {
                    return Progress::success(pt, tailed);
                }
                TailedState::ValueOnly(pt2, v) => {
                    if v.is_implicit_separator() {
                        pt = pt2;
                        tailed.values.push(v);
                        tailed.separator_count += 1;
                    } else {
                        tailed.values.push(v);
                        return Progress::success(pt2, tailed);
                    }
                    tailed.last_had_separator = false;
                }
                TailedState::ValueAndSeparator(pt2, v) => {
                    pt = pt2;
                    tailed.values.push(v);
                    tailed.separator_count += 1;
                    tailed.last_had_separator = true;
                }
            }
        }
    }
}

fn zero_or_more_implicitly_tailed_values<'s, F, T>(sep: &'static str, f: F) ->
    impl FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, Vec<T>>
    where F: Fn(&mut Master<'s>, Point<'s>) -> Progress<'s, T>,
          T: ImplicitSeparator
{
    map(zero_or_more_implicitly_tailed_append(Tailed::default(), sep, f), |t| t.values)
}

fn zero_or_more_implicitly_tailed_values_terminated<'s, F, T>(sep: &'static str, f: F) ->
    impl FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, (Vec<T>, bool)>
    where F: Fn(&mut Master<'s>, Point<'s>) -> Progress<'s, T>,
          T: ImplicitSeparator
{
    map(zero_or_more_implicitly_tailed_append(Tailed::default(), sep, f), |t| {
        (t.values, t.last_had_separator)
    })
}

fn one_or_more_tailed<'s, F, T>(sep: &'static str, f: F) ->
    impl FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, Tailed<T>>
    where F: Fn(&mut Master<'s>, Point<'s>) -> Progress<'s, T>
{
    move |pm, pt| {
        let mut tailed = Tailed::default();

        match parse_tailed(sep, &f, pm, pt) {
            TailedState::Nothing(pt, f) => {
                return Progress::failure(pt, f);
            }
            TailedState::ValueOnly(pt, v) => {
                tailed.values.push(v);
                return Progress::success(pt, tailed);
            }
            TailedState::ValueAndSeparator(pt, v) => {
                tailed.values.push(v);
                tailed.separator_count += 1;
                zero_or_more_tailed_append(tailed, sep, f)(pm, pt)
            }
        }
    }
}

fn one_or_more_tailed_values<'s, F, T>(sep: &'static str, f: F) ->
    impl FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, Vec<T>>
    where F: Fn(&mut Master<'s>, Point<'s>) -> Progress<'s, T>
{
    map(one_or_more_tailed(sep, f), |t| t.values)
}

fn optional_whitespace<'s>(ws: Vec<Whitespace>) -> impl FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, Vec<Whitespace>> {
    zero_or_more_append(ws, whitespace_core)
}

fn append_whitespace<'s>(ws: Vec<Whitespace>) -> impl FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, Vec<Whitespace>> {
    one_or_more_append(ws, whitespace_core)
}

fn concat_whitespace<'s, F, T>
    (mut ws: Vec<Whitespace>, parser: F)
     -> impl FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, (Option<T>, Vec<Whitespace>)>
    where F: FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, Option<(T, Vec<Whitespace>)>>
{
    move |pm, pt| {
        parser(pm, pt).map(|opt| {
            let val = opt.map(|(val, ws2)| {
                ws.extend(ws2);
                val
            });
            (val, ws)
        })
    }
}

// TODO: generic enough to move to library?
pub fn not<P, E, S, F, T>
    (parser: F, error: E)
     -> impl FnOnce(&mut peresil::ParseMaster<P, E, S>, P) -> peresil::Progress<P, (), E>
    where F: FnOnce(&mut peresil::ParseMaster<P, E, S>, P) -> peresil::Progress<P, T, E>,
          P: peresil::Point,
          E: peresil::Recoverable,
{
    move |pm, pt| {
        match parser(pm, pt) {
            peresil::Progress { status: peresil::Status::Success(_), .. } => {
                peresil::Progress::failure(pt, error)
            }
            peresil::Progress { status: peresil::Status::Failure(_), .. } => {
                peresil::Progress::success(pt, ())
            }
        }
    }
}

// --------------------------------------------------

fn item<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Item> {
    pm.alternate(pt)
        .one(map(attribute, Item::Attribute))
        .one(map(p_const, Item::Const))
        .one(map(extern_crate, Item::ExternCrate))
        .one(map(extern_block, Item::ExternBlock))
        .one(map(function, Item::Function))
        .one(map(item_macro_call, Item::MacroCall))
        .one(map(module, Item::Module))
        .one(map(p_enum, Item::Enum))
        .one(map(p_impl, Item::Impl))
        .one(map(p_static, Item::Static))
        .one(map(p_struct, Item::Struct))
        .one(map(p_trait, Item::Trait))
        .one(map(p_use, Item::Use))
        .one(map(type_alias, Item::TypeAlias))
        .one(map(whitespace, Item::Whitespace))
        .finish()
}

fn literal<'s>(expected: &'static str) -> impl Fn(&mut Master<'s>, Point<'s>) -> Progress<'s, &'s str> {
    move |_pm, pt| pt.consume_literal(expected).map_err(|_| Error::Literal(expected))
}

fn keyword<'s>(kw: &'static str) ->
    impl Fn(&mut Master<'s>, Point<'s>) -> Progress<'s, &'s str>
{
    move |pm, pt| {
        sequence!(pm, pt, {
            spt = point;
            _   = literal(kw);
            _   = not(ident, Error::ExpectedKeyword); // This is a hack; should be ident continuation
        }, |_, pt| spt.to(pt))
    }
}

fn comment<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Comment> {
    pm.alternate(pt)
        .one(comment_end_of_line)
        .one(comment_region)
        .finish()
}

fn comment_end_of_line<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Comment> {
    sequence!(pm, pt, {
        spt  = point;
        _    = literal("//");
        text = parse_until("\n");
    }, |_, pt| Comment { extent: ex(spt, pt), text })
}

fn comment_region<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Comment> {
    sequence!(pm, pt, {
        spt  = point;
        _    = literal("/*");
        text = parse_until("*/");
        _    = literal("*/");
    }, |_, pt| Comment { extent: ex(spt, pt), text })
}

fn function<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Function> {
    sequence!(pm, pt, {
        spt    = point;
        header = function_header;
        ws     = optional_whitespace(Vec::new());
        body   = block;
    }, |_, pt| Function {
        extent: ex(spt, pt),
        header,
        body,
        whitespace: ws
    })
}

fn function_header<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, FunctionHeader> {
    sequence!(pm, pt, {
        spt               = point;
        visibility        = optional(visibility);
        qualifiers        = function_qualifiers;
        _                 = keyword("fn");
        ws                = whitespace;
        name              = ident;
        ws                = optional_whitespace(ws);
        generics          = optional(generic_declarations);
        ws                = optional_whitespace(ws);
        arguments         = function_arglist;
        ws                = optional_whitespace(ws);
        (return_type, ws) = concat_whitespace(ws, optional(function_return_type));
        ws                = optional_whitespace(ws);
        (wheres, ws)      = concat_whitespace(ws, optional(where_clause));
    }, |_, pt| {
        FunctionHeader {
            extent: ex(spt, pt),
            visibility,
            qualifiers,
            name,
            generics,
            arguments,
            return_type,
            wheres: wheres.unwrap_or_else(Vec::new),
            whitespace: ws,
        }})
}

fn function_qualifiers<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, FunctionQualifiers> {
    sequence!(pm, pt, {
        spt       = point;
        is_const  = optional(function_qualifier_const);
        is_unsafe = optional(function_qualifier_unsafe);
        is_extern = optional(function_qualifier_extern);
    }, |_, pt| {
        let is_extern = is_extern;
        let (is_extern, abi) = match is_extern {
            Some((ex, abi)) => (Some(ex), abi),
            None => (None, None),
        };
        FunctionQualifiers { extent: ex(spt, pt), is_const, is_unsafe, is_extern, abi }
    })
}

fn function_qualifier_const<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Extent> {
    sequence!(pm, pt, {
        is_const = ext(keyword("const"));
        _x        = whitespace;
    }, |_, _| is_const)
}

fn function_qualifier_unsafe<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Extent> {
    sequence!(pm, pt, {
        is_unsafe = ext(keyword("unsafe"));
        _x        = whitespace;
    }, |_, _| is_unsafe)
}

fn function_qualifier_extern<'s>(pm: &mut Master<'s>, pt: Point<'s>) ->
    Progress<'s, (Extent, Option<String>)>
{
    sequence!(pm, pt, {
        is_extern = ext(keyword("extern"));
        _x        = whitespace;
        abi       = optional(string_literal);
        _x        = optional_whitespace(_x);
    }, |_, _| (is_extern, abi))
}

fn ident<'s>(_pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Ident> {
    let mut ci = pt.s.chars();
    let mut idx = 0;

    if let Some(c) = ci.next() {
        if UnicodeXID::is_xid_start(c) || c == '_' {
            idx += c.len_utf8();

            idx += ci.take_while(|&c| UnicodeXID::is_xid_continue(c)).map(|c| c.len_utf8()).sum();
        }
    }

    split_point_at_non_zero_offset(pt, idx, Error::ExpectedIdentifier)
        .and_then(pt, reject_keywords)
        .map(|extent| Ident { extent })
}

// Keywords should mostly match up with all the `literal(...)` calls
// Treat `self` as an identifier though.
fn reject_keywords((s, ex): (&str, Extent)) -> Result<Extent, Error> {
    match s {
        "as"       |
        "box"      |
        "break"    |
        "const"    |
        "continue" |
        "crate"    |
        "else"     |
        "enum"     |
        "extern"   |
        "fn"       |
        "for"      |
        "if"       |
        "impl"     |
        "in"       |
        "let"      |
        "loop"     |
        "match"    |
        "mod"      |
        "move"     |
        "mut"      |
        "pub"      |
        "ref"      |
        "return"   |
        "static"   |
        "struct"   |
        "trait"    |
        "type"     |
        "use"      |
        "unsafe"   |
        "where"    |
        "while"    => Err(Error::ExpectedIdentifier),
        _ => Ok(ex),
    }
}

fn split_point_at_non_zero_offset<'s>(pt: Point<'s>, idx: usize, e: Error) -> Progress<'s, (&'s str, Extent)> {
    if idx == 0 {
        Progress::failure(pt, e)
    } else {
        let (matched, tail) = pt.s.split_at(idx);
        let end = pt.offset + idx;
        let end_pt = Point { s: tail, offset: end };

        Progress::success(end_pt, (matched, (pt.offset, end)))
    }
}

fn generic_declarations<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, GenericDeclarations> {
    sequence!(pm, pt, {
        spt       = point;
        _x        = optional_whitespace(Vec::new());
        _         = literal("<");
        _x        = optional_whitespace(_x);
        lifetimes = zero_or_more_tailed_values(",", generic_declaration_lifetime);
        types     = zero_or_more_tailed_values(",", generic_declaration_type);
        _x        = optional_whitespace(_x);
        _         = literal(">");
    }, |_, pt| GenericDeclarations { extent: ex(spt, pt), lifetimes, types })
}

fn generic_declaration_lifetime<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, GenericDeclarationLifetime> {
    sequence!(pm, pt, {
        spt        = point;
        attributes = zero_or_more(struct_defn_field_attr);
        name       = lifetime;
        bounds     = optional(generic_declaration_lifetime_bounds);
    }, |_, pt| GenericDeclarationLifetime {
        extent: ex(spt, pt),
        attributes,
        name,
        bounds: bounds.unwrap_or_else(Vec::new),
    })
}

fn generic_declaration_lifetime_bounds<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Vec<Lifetime>> {
    sequence!(pm, pt, {
        _x     = optional_whitespace(Vec::new());
        _      = literal(":");
        _x     = optional_whitespace(_x);
        bounds = zero_or_more_tailed_values("+", lifetime);
    }, |_, _| bounds)
}

fn generic_declaration_type<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, GenericDeclarationType> {
    sequence!(pm, pt, {
        spt        = point;
        attributes = zero_or_more(struct_defn_field_attr);
        name       = ident;
        // Over-permissive; allows interleaving trait bounds and default types
        bounds     = optional(generic_declaration_type_addition);
    }, |_, pt| GenericDeclarationType { extent: ex(spt, pt), attributes, name, bounds })
}

fn generic_declaration_type_addition<'s>(pm: &mut Master<'s>, pt: Point<'s>) ->
    Progress<'s, GenericDeclarationTypeAddition>
{
    pm.alternate(pt)
        .one(map(generic_declaration_type_bounds, GenericDeclarationTypeAddition::Bounds))
        .one(map(generic_declaration_type_default, GenericDeclarationTypeAddition::Default))
        .finish()
}

fn generic_declaration_type_bounds<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TraitBounds> {
    sequence!(pm, pt, {
        _x     = optional_whitespace(Vec::new());
        _      = literal(":");
        _x     = optional_whitespace(_x);
        bounds = trait_bounds;
    }, |_, _| bounds)
}

fn generic_declaration_type_default<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Type> {
    sequence!(pm, pt, {
        _x  = optional_whitespace(Vec::new());
        _   = literal("=");
        _x  = optional_whitespace(_x);
        typ = typ;
    }, |_, _| typ)
}

fn function_arglist<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Vec<Argument>> {
    sequence!(pm, pt, {
        _        = literal("(");
        _x       = optional_whitespace(Vec::new());
        self_arg = optional(map(self_argument, Argument::SelfArgument));
        args     = zero_or_more_tailed_values_append(self_arg, ",", function_argument);
        _x       = optional_whitespace(_x);
        _        = literal(")");
    }, move |_, _| args)
}

fn self_argument<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, SelfArgument> {
    pm.alternate(pt)
        .one(map(self_argument_longhand, SelfArgument::Longhand))
        .one(map(self_argument_shorthand, SelfArgument::Shorthand))
        .finish()
}

fn self_argument_longhand<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, SelfArgumentLonghand> {
    sequence!(pm, pt, {
        spt  = point;
        name = ext(literal("self"));
        ws   = optional_whitespace(Vec::new());
        _    = literal(":");
        ws   = optional_whitespace(ws);
        typ  = typ;
        _    = optional(literal(","));
        ws   = optional_whitespace(ws);
    }, |_, pt| SelfArgumentLonghand {
        extent: ex(spt, pt),
        name: Ident { extent: name },
        typ,
        whitespace: ws,
    })
}

fn self_argument_shorthand<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, SelfArgumentShorthand> {
    sequence!(pm, pt, {
        spt       = point;
        qualifier = optional(self_argument_qualifier);
        name      = ext(literal("self"));
        _         = optional(literal(","));
        ws        = optional_whitespace(Vec::new());
    }, |_, pt| SelfArgumentShorthand {
        extent: ex(spt, pt),
        qualifier,
        name: Ident { extent: name },
        whitespace: ws,
    })
}

fn self_argument_qualifier<'s>(pm: &mut Master<'s>, pt: Point<'s>) ->
    Progress<'s, SelfArgumentShorthandQualifier>
{
    pm.alternate(pt)
        .one(map(typ_reference_kind, SelfArgumentShorthandQualifier::Reference))
        .one(map(self_argument_mut, SelfArgumentShorthandQualifier::Mut))
        .finish()
}

fn self_argument_mut<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Extent> {
    sequence!(pm, pt, {
        is_mut = ext(keyword("mut"));
        _x     = optional_whitespace(Vec::new());
    }, |_, _| is_mut)
}

fn function_argument<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Argument> {
    sequence!(pm, pt, {
        name = pattern;
        ws   = optional_whitespace(Vec::new());
        _    = literal(":");
        ws   = optional_whitespace(ws);
        typ  = typ;
    }, |_, _| Argument::Named(NamedArgument { name, typ, whitespace: ws }))
}

fn function_return_type<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, (Type, Vec<Whitespace>)> {
    sequence!(pm, pt, {
        _   = literal("->");
        ws  = optional_whitespace(Vec::new());
        typ = typ;
    }, |_, _| (typ, ws))
}

fn where_clause<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, (Vec<Where>, Vec<Whitespace>)> {
    sequence!(pm, pt, {
        _  = keyword("where");
        ws = whitespace;
        w  = one_or_more_tailed_values(",", where_clause_item);
    }, |_, _| (w, ws))
}

fn where_clause_item<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Where> {
    pm.alternate(pt)
        .one(map(where_lifetime, Where::Lifetime))
        .one(map(where_type, Where::Type))
        .finish()
}

fn where_lifetime<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, WhereLifetime> {
    sequence!(pm, pt, {
        spt    = point;
        name   = lifetime;
        bounds = generic_declaration_lifetime_bounds;
    }, |_, pt| WhereLifetime { extent: ex(spt, pt), name, bounds  })
}

fn where_type<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, WhereType> {
    sequence!(pm, pt, {
        spt    = point;
        name   = typ;
        bounds = generic_declaration_type_bounds;
    }, |_, pt| WhereType { extent: ex(spt, pt), name, bounds  })
}

fn trait_bounds<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TraitBounds> {
    sequence!(pm, pt, {
        spt = point;
        types = zero_or_more_tailed_values("+", trait_bound);
    }, |_, pt| TraitBounds { extent: ex(spt, pt), types })
}

fn trait_bound<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TraitBound> {
    pm.alternate(pt)
        .one(map(trait_bound_lifetime, TraitBound::Lifetime))
        .one(map(trait_bound_normal, TraitBound::Normal))
        .one(map(trait_bound_relaxed, TraitBound::Relaxed))
        .finish()
}

fn trait_bound_lifetime<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TraitBoundLifetime> {
    sequence!(pm, pt, {
        spt      = point;
        lifetime = lifetime;
    }, |_, pt| TraitBoundLifetime { extent: ex(spt, pt), lifetime })
}

fn trait_bound_normal<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TraitBoundNormal> {
    sequence!(pm, pt, {
        spt = point;
        typ = trait_bound_normal_child;
    }, |_, pt| TraitBoundNormal { extent: ex(spt, pt), typ })
}

fn trait_bound_normal_child<'s>(pm: &mut Master<'s>, pt: Point<'s>) ->
    Progress<'s, TraitBoundType>
{
    pm.alternate(pt)
        .one(map(typ_named, TraitBoundType::Named))
        .one(map(typ_higher_ranked_trait_bounds, TraitBoundType::HigherRankedTraitBounds))
        .finish()
}

fn trait_bound_relaxed<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TraitBoundRelaxed> {
    sequence!(pm, pt, {
        spt = point;
        _   = literal("?");
        ws  = optional_whitespace(Vec::new());
        typ = trait_bound_normal_child;
    }, |_, pt| TraitBoundRelaxed { extent: ex(spt, pt), typ, whitespace: ws })
}

fn block<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Block> {
    sequence!(pm, pt, {
        spt               = point;
        _                 = literal("{");
        ws                = optional_whitespace(Vec::new());
        (mut stmts, term) = zero_or_more_implicitly_tailed_values_terminated(";", statement);
        ws                = optional_whitespace(ws);
        _                 = literal("}");
    }, |_, pt| {
        let expr = if !term && stmts.last().map_or(false, Statement::is_expression) {
            stmts.pop().and_then(Statement::into_expression)
        } else {
            None
        };

        Block {
            extent: ex(spt, pt),
            statements: stmts,
            expression: expr,
            whitespace: ws,
        }
    })
}

fn statement<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Statement> {
    sequence!(pm, pt, {
        _x   = optional(whitespace);
        expr = statement_inner;
        _x   = optional(whitespace);
    }, |_, _| expr)
}

fn statement_inner<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Statement> {
    pm.alternate(pt)
        .one(map(expression, Statement::Expression))
        .one(map(item, Statement::Item))
        .finish()
}

// idea: trait w/associated types to avoid redefin fn types?

impl ImplicitSeparator for Statement {
    fn is_implicit_separator(&self) -> bool {
        match *self {
            Statement::Expression(Expression::If(_))          |
            Statement::Expression(Expression::IfLet(_))       |
            Statement::Expression(Expression::ForLoop(_))     |
            Statement::Expression(Expression::Loop(_))        |
            Statement::Expression(Expression::While(_))       |
            Statement::Expression(Expression::WhileLet(_))    |
            Statement::Expression(Expression::Match(_))       |
            Statement::Expression(Expression::UnsafeBlock(_)) |
            Statement::Expression(Expression::Block(_))       |
            Statement::Expression(Expression::MacroCall(MacroCall { args: MacroCallArgs::Curly(_), .. })) |
            Statement::Item(_)                                => true,

            Statement::Expression(_) => false,
        }
    }
}

fn expression<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Expression> {
    let spt = pt;
    let (pt, _) = try_parse!(optional(whitespace)(pm, pt));
    let (pt, mut expression) = try_parse!({
        pm.alternate(pt)
            .one(map(expr_if, Expression::If))
            .one(map(expr_if_let, Expression::IfLet))
            .one(map(expr_for_loop, Expression::ForLoop))
            .one(map(expr_loop, Expression::Loop))
            .one(map(expr_while, Expression::While))
            .one(map(expr_while_let, Expression::WhileLet))
            .one(map(expr_match, Expression::Match))
            .one(map(expr_unsafe_block, Expression::UnsafeBlock))
            .one(map(expr_block, Expression::Block))
            .one(map(expr_macro_call, Expression::MacroCall))
            .one(map(expr_let, Expression::Let))
            .one(expr_tuple_or_parenthetical)
            .one(map(expr_range, Expression::Range))
            .one(map(expr_array, Expression::Array))
            .one(map(character_literal, Expression::Character))
            .one(map(string_literal, Expression::String))
            .one(map(expr_closure, Expression::Closure))
            .one(map(expr_return, Expression::Return))
            .one(map(expr_continue, Expression::Continue))
            .one(map(expr_break, Expression::Break))
            .one(map(number_literal, Expression::Number))
            .one(map(expr_reference, Expression::Reference))
            .one(map(expr_dereference, Expression::Dereference))
            .one(map(expr_unary, Expression::Unary))
            .one(map(expr_box, Expression::Box))
            .one(map(expr_byte, Expression::Byte))
            .one(map(expr_byte_string, Expression::ByteString))
            .one(map(expr_disambiguation, Expression::Disambiguation))
            .one(map(expr_value, Expression::Value))
            .finish()
    });

    let mut pt = pt;
    loop {
        let (pt2, tail) = try_parse!(optional(expression_tail)(pm, pt));
        pt = pt2;
        match tail {
            Some(ExpressionTail::AsType { typ, whitespace }) => {
                expression = Expression::AsType(AsType {
                    extent: ex(spt, pt),
                    value: Box::new(expression),
                    typ,
                    whitespace,
                })
            }
            Some(ExpressionTail::Binary { op, rhs, whitespace }) => {
                expression = Expression::Binary(Binary {
                    extent: ex(spt, pt),
                    op,
                    lhs: Box::new(expression),
                    rhs,
                    whitespace,
                })
            }
            Some(ExpressionTail::FieldAccess { field }) => {
                expression = Expression::FieldAccess(FieldAccess {
                    extent: ex(spt, pt),
                    value: Box::new(expression),
                    field: field,
                })
            }
            Some(ExpressionTail::Call { args }) => {
                expression = Expression::Call(Call {
                    extent: ex(spt, pt),
                    target: Box::new(expression),
                    args: args
                })
            }
            Some(ExpressionTail::Range { rhs }) => {
                expression = Expression::Range(Range {
                    extent: ex(spt, pt),
                    lhs: Some(Box::new(expression)),
                    rhs
                })
            }
            Some(ExpressionTail::Slice { range, whitespace }) => {
                expression = Expression::Slice(Slice {
                    extent: ex(spt, pt),
                    target: Box::new(expression),
                    range,
                    whitespace,
                })
            }
            Some(ExpressionTail::TryOperator) => {
                expression = Expression::TryOperator(TryOperator {
                    extent: ex(spt, pt),
                    target: Box::new(expression),
                })
            }
            None => break,
        }
    }

    Progress::success(pt, expression)
}

fn expr_macro_call<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, MacroCall> {
    sequence!(pm, pt, {
        spt  = point;
        name = ident;
        _    = literal("!");
        _x   = optional_whitespace(Vec::new());
        arg  = optional(ident);
        _x   = optional_whitespace(_x);
        args = expr_macro_call_args;
    }, |_, pt| MacroCall { extent: ex(spt, pt), name, arg, args })
}

fn expr_macro_call_args<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, MacroCallArgs> {
    pm.alternate(pt)
        .one(map(expr_macro_call_paren, MacroCallArgs::Paren))
        .one(map(expr_macro_call_square, MacroCallArgs::Square))
        .one(map(expr_macro_call_curly, MacroCallArgs::Curly))
        .finish()
}

fn expr_macro_call_paren<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Extent> {
    sequence!(pm, pt, {
        _    = literal("(");
        args = parse_nested_tokens_until(Token::is_left_paren, Token::is_right_paren);
        _    = literal(")");
    }, |_, _| args)
}

fn expr_macro_call_square<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Extent> {
    sequence!(pm, pt, {
        _    = literal("[");
        args = parse_nested_tokens_until(Token::is_left_square, Token::is_right_square);
        _    = literal("]");
    }, |_, _| args)
}

fn expr_macro_call_curly<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Extent> {
    sequence!(pm, pt, {
        _    = literal("{");
        args = parse_nested_tokens_until(Token::is_left_curly, Token::is_right_curly);
        _    = literal("}");
    }, |_, _| args)
}

// TODO: There's a good amount of duplication here; revisit and DRY up
// Mostly in the required ; for paren and square...
fn item_macro_call<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, MacroCall> {
    sequence!(pm, pt, {
        spt  = point;
        name = ident;
        _    = literal("!");
        _x   = optional_whitespace(Vec::new());
        arg  = optional(ident);
        _x   = optional_whitespace(_x);
        args = item_macro_call_args;
    }, |_, pt| MacroCall { extent: ex(spt, pt), name, arg, args })
}

fn item_macro_call_args<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, MacroCallArgs> {
    pm.alternate(pt)
        .one(map(item_macro_call_paren, MacroCallArgs::Paren))
        .one(map(item_macro_call_square, MacroCallArgs::Square))
        .one(map(item_macro_call_curly, MacroCallArgs::Curly))
        .finish()
}

fn item_macro_call_paren<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Extent> {
    sequence!(pm, pt, {
        _    = literal("(");
        args = parse_nested_tokens_until(Token::is_left_paren, Token::is_right_paren);
        _    = literal(")");
        _    = literal(";");
    }, |_, _| args)
}

fn item_macro_call_square<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Extent> {
    sequence!(pm, pt, {
        _    = literal("[");
        args = parse_nested_tokens_until(Token::is_left_square, Token::is_right_square);
        _    = literal("]");
        _    = literal(";");
    }, |_, _| args)
}

fn item_macro_call_curly<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Extent> {
    sequence!(pm, pt, {
        _    = literal("{");
        args = parse_nested_tokens_until(Token::is_left_curly, Token::is_right_curly);
        _    = literal("}");
    }, |_, _| args)
}

fn expr_let<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Let> {
    sequence!(pm, pt, {
        spt         = point;
        _           = keyword("let");
        ws          = whitespace;
        pattern     = pattern;
        ws          = optional_whitespace(ws);
        (typ, ws)   = concat_whitespace(ws, optional(expr_let_type));
        ws          = optional_whitespace(ws);
        (value, ws) = concat_whitespace(ws, optional(expr_let_rhs));
    }, |_, pt| Let {
        extent: ex(spt, pt),
        pattern,
        typ,
        value: value.map(Box::new),
        whitespace: ws,
    })
}

fn expr_let_type<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, (Type, Vec<Whitespace>)> {
    sequence!(pm, pt, {
        ws  = optional_whitespace(Vec::new());
        _   = literal(":");
        ws  = optional_whitespace(ws);
        typ = typ;
    }, |_, _| (typ, ws))
}

fn expr_let_rhs<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, (Expression, Vec<Whitespace>)> {
    sequence!(pm, pt, {
        _     = literal("=");
        ws    = optional_whitespace(Vec::new());
        value = expression;
    }, |_, _| (value, ws))
}

fn expr_if<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, If> {
    sequence!(pm, pt, {
        spt               = point;
        _                 = keyword("if");
        ws                = whitespace;
        (condition, body) = expr_followed_by_block;
        more              = zero_or_more(expr_if_else_if);
        (else_body, ws)   = concat_whitespace(ws, optional(expr_if_else_end));
    }, move |_, pt| If {
        extent: ex(spt, pt),
        condition: Box::new(condition),
        body: Box::new(body),
        more,
        else_body: else_body.map(Box::new),
        whitespace: ws,
    })
}

fn expr_if_else_if<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, If> {
    sequence!(pm, pt, {
        _x   = optional(whitespace);
        _    = keyword("else");
        _x   = optional(whitespace);
        tail = expr_if;
    }, |_, _| tail)
}

fn expr_if_else_end<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, (Block, Vec<Whitespace>)> {
    sequence!(pm, pt, {
        ws        = optional_whitespace(Vec::new());
        _         = keyword("else");
        ws        = optional_whitespace(ws);
        else_body = block;
    }, |_, _| (else_body, ws))
}

// To check if a given subexpression should re-allow struct literals,
// test something like this with the official compiler:
//
// ```rust
// fn main() {
//     struct Foo {a: u8}
//     if $parent_expression Foo {a: 42} {}
// }
// ```
//
// In general, anything that is inside some kind of enclosing
// container should re-enable them because it is no longer ambiguous.
fn allow_struct_literals<'s, F, T>(parser: F) ->
    impl FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, T>
    where F: FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, T>
{
    set_ignore_struct_literals(parser, false)
}

fn disallow_struct_literals<'s, F, T>(parser: F) ->
    impl FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, T>
    where F: FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, T>
{
    set_ignore_struct_literals(parser, true)
}

fn set_ignore_struct_literals<'s, F, T>(parser: F, ignore: bool) ->
    impl FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, T>
    where F: FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, T>
{
    move |pm, pt| {
        let old = pm.state.ignore_struct_literals;
        pm.state.ignore_struct_literals = ignore;

        let res = parser(pm, pt);

        pm.state.ignore_struct_literals = old;

        res
    }
}

fn expr_followed_by_block<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, (Expression, Block)> {
    sequence!(pm, pt, {
        condition = disallow_struct_literals(expression);
        _x        = optional(whitespace);
        body      = block;
    }, |_, _| (condition, body))
}

fn expr_for_loop<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, ForLoop> {
    sequence!(pm, pt, {
        spt          = point;
        label        = optional(loop_label);
        _            = keyword("for");
        ws           = append_whitespace(Vec::new());
        pattern      = pattern;
        ws           = append_whitespace(ws);
        _            = keyword("in");
        ws           = append_whitespace(ws);
        (iter, body) = expr_followed_by_block;
    }, |_, pt| ForLoop {
        extent: ex(spt, pt),
        label,
        pattern,
        iter: Box::new(iter),
        body: Box::new(body),
        whitespace: ws,
    })
}

fn loop_label<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Lifetime> {
    sequence!(pm, pt, {
        lifetime = lifetime;
        _x       = optional_whitespace(Vec::new());
        _        = literal(":");
        _x       = optional_whitespace(Vec::new());
    }, |_, _| lifetime)
}

fn expr_loop<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Loop> {
    sequence!(pm, pt, {
        spt   = point;
        label = optional(loop_label);
        _     = keyword("loop");
        ws    = optional_whitespace(Vec::new());
        body  = block;
    }, |_, pt| Loop { extent: ex(spt, pt), label, body: Box::new(body), whitespace: ws })
}

fn expr_if_let<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, IfLet> {
    sequence!(pm, pt, {
        spt           = point;
        _             = keyword("if");
        ws            = whitespace;
        _             = keyword("let");
        ws            = append_whitespace(ws);
        pattern       = pattern;
        ws            = optional_whitespace(ws);
        _             = literal("=");
        ws            = optional_whitespace(ws);
        (value, body) = expr_followed_by_block;
    }, |_, pt| IfLet {
        extent: ex(spt, pt),
        pattern,
        value: Box::new(value),
        body: Box::new(body),
        whitespace: ws,
    })
}

fn expr_while<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, While> {
    sequence!(pm, pt, {
        spt           = point;
        label         = optional(loop_label);
        _             = keyword("while");
        ws            = whitespace;
        (value, body) = expr_followed_by_block;
    }, |_, pt| While {
        extent: ex(spt, pt),
        label,
        value: Box::new(value),
        body: Box::new(body),
        whitespace: ws,
    })
}

fn expr_while_let<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, WhileLet> {
    sequence!(pm, pt, {
        spt           = point;
        label         = optional(loop_label);
        _             = keyword("while");
        ws            = whitespace;
        _             = keyword("let");
        ws            = append_whitespace(ws);
        pattern       = pattern;
        ws            = optional_whitespace(ws);
        _             = literal("=");
        ws            = optional_whitespace(ws);
        (value, body) = expr_followed_by_block;
    }, |_, pt| WhileLet {
        extent: ex(spt, pt),
        label,
        pattern,
        value: Box::new(value),
        body: Box::new(body),
        whitespace: ws,
    })
}

impl ImplicitSeparator for MatchArm {
    fn is_implicit_separator(&self) -> bool {
        match self.hand {
            MatchHand::Brace(..) => true,
            MatchHand::Expression(..) => false,
        }
    }
}

fn expr_match<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Match> {
    sequence!(pm, pt, {
        spt  = point;
        _    = keyword("match");
        ws   = append_whitespace(Vec::new());
        head = disallow_struct_literals(expression);
        ws   = optional_whitespace(ws);
        _    = literal("{");
        arms = zero_or_more_implicitly_tailed_values(",", match_arm);
        ws   = optional_whitespace(ws);
        _    = literal("}");
    }, |_, pt| Match { extent: ex(spt, pt), head: Box::new(head), arms, whitespace: ws })
}

fn match_arm<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, MatchArm> {
    sequence!(pm, pt, {
        spt        = point;
        ws         = optional_whitespace(Vec::new());
        attributes = zero_or_more(struct_defn_field_attr);
        pattern    = one_or_more_tailed_values("|", pattern);
        ws         = optional_whitespace(ws);
        guard      = optional(match_arm_guard);
        ws         = optional_whitespace(ws);
        _          = literal("=>");
        ws         = optional_whitespace(ws);
        hand       = match_arm_hand;
        ws         = optional_whitespace(ws);
    }, |_, pt| MatchArm { extent: ex(spt, pt), attributes, pattern, guard, hand, whitespace: ws })
}

fn match_arm_guard<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Expression> {
    sequence!(pm, pt, {
        _     = keyword("if");
        _x    = whitespace;
        guard = allow_struct_literals(expression);
    }, |_, _| guard)
}

fn match_arm_hand<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, MatchHand> {
    pm.alternate(pt)
        .one(map(match_arm_hand_brace, MatchHand::Brace))
        .one(map(match_arm_hand_expression, MatchHand::Expression))
        .finish()
}

fn match_arm_hand_expression<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Expression> {
    sequence!(pm, pt, {
        body = allow_struct_literals(expression);
    }, |_, _| body)
}

fn match_arm_hand_brace<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Expression> {
    sequence!(pm, pt, {
        body = allow_struct_literals(expr_block);
    }, |_, _| Expression::Block(body))
}

fn expr_tuple_or_parenthetical<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Expression> {
    sequence!(pm, pt, {
        spt    = point;
        _      = literal("(");
        _x     = optional_whitespace(Vec::new());
        values = allow_struct_literals(zero_or_more_tailed(",", expression));
        _x     = optional_whitespace(_x);
        _      = literal(")");
    }, move |_, pt| {
        let extent = ex(spt, pt);
        let values = values;
        let Tailed { mut values, separator_count, .. } = values;
        match (values.len(), separator_count) {
            (1, 0) => Expression::Parenthetical(Parenthetical {
                extent,
                expression: Box::new(values.pop().expect("Must have one parenthesized value")),
            }),
            _ => Expression::Tuple(Tuple {
                extent,
                members: values,
            }),
        }
    })
}

fn expr_range<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Range> {
    sequence!(pm, pt, {
        spt = point;
        _x  = optional_whitespace(Vec::new());
        _   = literal("..");
        _x  = optional_whitespace(_x);
        rhs = optional(expression);
    }, |_, pt| Range { extent: ex(spt, pt), lhs: None, rhs: rhs.map(Box::new) } )
}

fn expr_array<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Array> {
    pm.alternate(pt)
        .one(map(expr_array_explicit, Array::Explicit))
        .one(map(expr_array_repeated, Array::Repeated))
        .finish()
}

fn expr_array_explicit<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, ArrayExplicit> {
    sequence!(pm, pt, {
        spt    = point;
        _      = literal("[");
        _x     = optional_whitespace(Vec::new());
        values = allow_struct_literals(zero_or_more_tailed_values(",", expression));
        _x     = optional_whitespace(_x);
        _      = literal("]");
    }, |_, pt| ArrayExplicit { extent: ex(spt, pt), values })
}

fn expr_array_repeated<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, ArrayRepeated> {
    sequence!(pm, pt, {
        spt   = point;
        _     = literal("[");
        ws    = optional_whitespace(Vec::new());
        value = allow_struct_literals(expression);
        ws    = optional_whitespace(ws);
        _     = literal(";");
        ws    = optional_whitespace(ws);
        count = expression;
        ws    = optional_whitespace(ws);
        _     = literal("]");
    }, |_, pt| ArrayRepeated {
        extent: ex(spt, pt),
        value: Box::new(value),
        count: Box::new(count),
        whitespace: ws,
    })
}

fn character_literal<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Character> {
    sequence!(pm, pt, {
        spt   = point;
        _     = literal("'");
        value = ext(char_char);
        _     = literal("'");
    }, |_, pt| Character { extent: ex(spt, pt), value })
}

fn char_char<'s>(_pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, &'s str> {
    let res = |i| {
        let (head, tail) = pt.s.split_at(i);
        let pt = Point { s: tail, offset: pt.offset + i };
        Progress::success(pt, head)
    };

    let mut escaped = false;
    for (i, c) in pt.s.char_indices() {
        match (escaped, c) {
            (true, _) => escaped = false,
            (false, '\\') => escaped = true,
            (false, '\'') => return res(i),
            (false, _) => { /* Next char */ },
        }
    }

    res(pt.s.len())
}

fn string_literal<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, String> {
    pm.alternate(pt)
        .one(string_literal_normal)
        .one(string_literal_raw)
        .finish()
}

fn string_literal_normal<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, String> {
    sequence!(pm, pt, {
        spt   = point;
        _     = literal("\"");
        value = ext(str_char);
        _     = literal("\"");
    }, |_, pt| String { extent: ex(spt, pt), value })
}

fn str_char<'s>(_pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, &'s str> {
    let res = |i| {
        let (head, tail) = pt.s.split_at(i);
        let pt = Point { s: tail, offset: pt.offset + i };
        Progress::success(pt, head)
    };

    let mut escaped = false;
    for (i, c) in pt.s.char_indices() {
        match (escaped, c) {
            (true, _) => escaped = false,
            (false, '\\') => escaped = true,
            (false, '"') => return res(i),
            (false, _) => { /* Next char */ },
        }
    }

    res(pt.s.len())
}

fn string_literal_raw<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, String> {
    sequence!(pm, pt, {
        spt   = point;
        _     = literal("r");
        h     = zero_or_more(literal("#"));
        _     = literal(r#"""#);
        value = ext(raw_raw(h.len()));
    }, |_, pt| String { extent: ex(spt, pt), value })
}

fn raw_raw<'s>(hashes: usize) -> impl Fn(&mut Master<'s>, Point<'s>) -> Progress<'s, &'s str> {
    let mut s = r#"""#.to_string();
    for _ in 0..hashes { s.push('#') };

    move |_, pt| {
        match pt.s.find(&s) {
            Some(end) => {
                let (str_content, quote_tail) = pt.s.split_at(end);
                let (_quotes, tail) = quote_tail.split_at(s.len());
                let pt = Point { s: tail, offset: pt.offset + end + s.len() };
                Progress::success(pt, str_content)
            }
            None => {
                Progress::failure(pt, Error::UnterminatedRawString)
            }
        }
    }
}

fn expr_byte<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Byte> {
    sequence!(pm, pt, {
        spt   = point;
        _     = literal("b");
        value = character_literal;
    }, |_, pt| Byte { extent: ex(spt, pt), value })
}

fn expr_byte_string<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, ByteString> {
    sequence!(pm, pt, {
        spt   = point;
        _     = literal("b");
        value = string_literal;
    }, |_, pt| ByteString { extent: ex(spt, pt), value })
}

fn expr_closure<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Closure> {
    sequence!(pm, pt, {
        spt                 = point;
        mov                 = optional(keyword("move"));
        ws                  = optional_whitespace(Vec::new());
        _                   = literal("|");
        ws                  = optional_whitespace(ws);
        args                = zero_or_more_tailed_values(",", expr_closure_arg);
        ws                  = optional_whitespace(ws);
        _                   = literal("|");
        (return_type, body) = expr_closure_return;
    }, |_, pt| Closure {
        extent: ex(spt, pt),
        is_move: mov.is_some(),
        args,
        return_type,
        body: Box::new(body),
        whitespace: ws,
    })
}

fn expr_closure_arg<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, ClosureArg> {
    sequence!(pm, pt, {
        name      = pattern;
        (typ, ws) = concat_whitespace(Vec::new(), optional(expr_closure_arg_type));
    }, |_, _| ClosureArg { name, typ, whitespace: ws })
}

fn expr_closure_arg_type<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, (Type, Vec<Whitespace>)> {
    sequence!(pm, pt, {
        ws  = optional_whitespace(Vec::new());
        _   = literal(":");
        ws  = optional_whitespace(ws);
        typ = typ;
    }, |_, _| (typ, ws))
}

fn expr_closure_return<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, (Option<Type>, Expression)> {
    pm.alternate(pt)
        .one(expr_closure_return_explicit)
        .one(expr_closure_return_inferred)
        .finish()
}

fn expr_closure_return_explicit<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, (Option<Type>, Expression)> {
    sequence!(pm, pt, {
        _x   = optional_whitespace(Vec::new());
        _    = literal("->");
        _x   = optional_whitespace(_x);
        typ  = typ;
        _x   = optional_whitespace(_x);
        body = expr_closure_return_body;
    }, |_, _| (Some(typ), body))
}

fn expr_closure_return_body<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Expression> {
    pm.alternate(pt)
        .one(expr_tuple_or_parenthetical)
        .one(map(expr_block, Expression::Block))
        .finish()
}

fn expr_closure_return_inferred<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, (Option<Type>, Expression)> {
    map(expression, |body| (None, body))(pm, pt)
}

fn expr_return<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Return> {
    sequence!(pm, pt, {
        spt   = point;
        _     = keyword("return");
        ws    = optional_whitespace(Vec::new());
        value = optional(expression);
    }, |_, pt| Return {
        extent: ex(spt, pt),
        value: value.map(Box::new),
        whitespace: ws,
    })
}

fn expr_continue<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Continue> {
    sequence!(pm, pt, {
        spt   = point;
        _     = keyword("continue");
        ws    = optional_whitespace(Vec::new());
        label = optional(lifetime);
    }, |_, pt| Continue { extent: ex(spt, pt), label, whitespace: ws })
}

fn expr_break<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Break> {
    sequence!(pm, pt, {
        spt   = point;
        _     = keyword("break");
        ws    = optional_whitespace(Vec::new());
        label = optional(lifetime);
    }, |_, pt| Break { extent: ex(spt, pt), label, whitespace: ws })
}

fn expr_block<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Box<Block>> {
    block(pm, pt).map(Box::new)
}

fn expr_unsafe_block<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, UnsafeBlock> {
    sequence!(pm, pt, {
        spt   = point;
        _     = keyword("unsafe");
        ws    = optional_whitespace(Vec::new());
        body = block;
    }, |_, pt| UnsafeBlock { extent: ex(spt, pt), body: Box::new(body), whitespace: ws })
}

fn number_literal<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Number> {
    pm.alternate(pt)
        .one(map(number_literal_binary, Number::Binary))
        .one(map(number_literal_hexadecimal, Number::Hexadecimal))
        .one(map(number_literal_octal, Number::Octal))
        // Must be last as all the others start with `0`
        .one(map(number_literal_decimal, Number::Decimal))
        .finish()
}

fn number_literal_decimal<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, NumberDecimal> {
    sequence!(pm, pt, {
        spt                 = point;
        (decimal, fraction) = number_literal_decimal_fraction(10);
        exponent            = optional(number_literal_exponent);
        suffix              = optional(number_literal_suffix);
    }, |_, pt| NumberDecimal { extent: ex(spt, pt), decimal, fraction, exponent, suffix })
}

fn number_literal_binary<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, NumberBinary> {
    number_literal_prefixed("0b", 2, |extent, decimal, fraction, exponent, suffix| {
        NumberBinary { extent, decimal, fraction, exponent, suffix }
    })(pm, pt)
}

fn number_literal_hexadecimal<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, NumberHexadecimal> {
    number_literal_prefixed("0x", 16, |extent, decimal, fraction, exponent, suffix| {
        NumberHexadecimal { extent, decimal, fraction, exponent, suffix }
    })(pm, pt)
}

fn number_literal_octal<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, NumberOctal> {
    number_literal_prefixed("0o", 8, |extent, decimal, fraction, exponent, suffix| {
        NumberOctal { extent, decimal, fraction, exponent, suffix }
    })(pm, pt)
}

fn number_literal_prefixed<'s, C, T>(prefix: &'static str, base: u32, constructor: C) ->
    impl Fn(&mut Master<'s>, Point<'s>) -> Progress<'s, T>
    where C: Fn(Extent, Extent, Option<Extent>, Option<Extent>, Option<NumberSuffix>) -> T
{
    move |pm, pt| {
        sequence!(pm, pt, {
            spt                 = point;
            _                   = literal(prefix);
            _                   = zero_or_more(literal("_"));
            (decimal, fraction) = number_literal_decimal_fraction(base);
            exponent            = optional(number_literal_exponent);
            suffix              = optional(number_literal_suffix);
        }, |_, pt| constructor(ex(spt, pt), decimal, fraction, exponent, suffix))
    }
}

fn number_literal_decimal_fraction<'s>(base: u32) ->
    impl FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, (Extent, Option<Extent>)>
{
    move |pm, pt| {
        sequence!(pm, pt, {
            decimal  = number_literal_base(base);
            fraction = optional(number_literal_fraction(base));
        }, |_, _| (decimal, fraction))
    }
}

fn number_literal_fraction<'s>(base: u32) ->
    impl FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, Extent>
{
    move |pm, pt| {
        sequence!(pm, pt, {
            _        = literal(".");
            fraction = number_literal_base(base);
        }, |_, _| fraction)
    }
}

fn number_literal_base<'s>(base: u32) ->
    impl FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, Extent>
{
    move |_, pt| {
        let idx = pt.s.chars().enumerate()
            // Disallow leading underscores here
            .take_while(|&(i, c)| c.is_digit(base) || (i != 0 && c == '_'))
            .map(|(_, c)| c.len_utf8())
            .sum();

        split_point_at_non_zero_offset(pt, idx, Error::ExpectedNumber)
            .map(|(_, ex)| ex)
    }
}

fn number_literal_exponent<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Extent> {
    sequence!(pm, pt, {
        _     = literal("e");
        value = number_literal_base(10);
    }, |_, _| value)
}

fn number_literal_suffix<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, NumberSuffix> {
    pm.alternate(pt)
        .one(map(literal("f32"), |_| NumberSuffix::F32))
        .one(map(literal("f64"), |_| NumberSuffix::F64))
        .one(map(literal("u8"), |_| NumberSuffix::U8))
        .one(map(literal("u16"), |_| NumberSuffix::U16))
        .one(map(literal("u32"), |_| NumberSuffix::U32))
        .one(map(literal("u64"), |_| NumberSuffix::U64))
        .one(map(literal("usize"), |_| NumberSuffix::Usize))
        .one(map(literal("i8"), |_| NumberSuffix::I8))
        .one(map(literal("i16"), |_| NumberSuffix::I16))
        .one(map(literal("i32"), |_| NumberSuffix::I32))
        .one(map(literal("i64"), |_| NumberSuffix::I64))
        .one(map(literal("isize"), |_| NumberSuffix::Isize))
        .finish()
}

fn pure_number<'s>(_pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Extent> {
    let idx = pt.s.chars().take_while(|&c| c.is_digit(10)).map(|c| c.len_utf8()).sum();

    split_point_at_non_zero_offset(pt, idx, Error::ExpectedNumber).map(|(_, ex)| ex)
}

fn expr_reference<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Reference> {
    sequence!(pm, pt, {
        spt     = point;
        _       = literal("&");
        ws      = optional_whitespace(Vec::new());
        mutable = optional(ext(keyword("mut")));
        value   = expression;
    }, |_, pt| Reference {
        extent: ex(spt, pt),
        mutable,
        value: Box::new(value),
        whitespace: ws,
    } )
}

fn expr_dereference<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Dereference> {
    sequence!(pm, pt, {
        spt     = point;
        _       = literal("*");
        ws      = optional_whitespace(Vec::new());
        value   = expression;
    }, |_, pt| Dereference {
        extent: ex(spt, pt),
        value: Box::new(value),
        whitespace: ws,
    })
}

fn expr_unary<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Unary> {
    sequence!(pm, pt, {
        spt   = point;
        op    = expr_unary_op;
        ws    = optional_whitespace(Vec::new());
        value = expression;
    }, |_, pt| Unary {
        extent: ex(spt, pt),
        op,
        value: Box::new(value),
        whitespace: ws
    })
}

fn expr_unary_op<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, UnaryOp> {
    pm.alternate(pt)
        .one(map(literal("!"), |_| UnaryOp::Not))
        .one(map(literal("-"), |_| UnaryOp::Negate))
        .finish()
}

fn expr_box<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, ExpressionBox> {
    sequence!(pm, pt, {
        spt   = point;
        _     = keyword("box");
        ws    = whitespace;
        value = expression;
    }, |_, pt| ExpressionBox {
        extent: ex(spt, pt),
        value: Box::new(value),
        whitespace: ws
    })
}

fn expr_value<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Value> {
    if pm.state.ignore_struct_literals {
        sequence!(pm, pt, {
            spt  = point;
            name = pathed_ident;
        }, |_, pt| Value { extent: ex(spt, pt), name, literal: None })
    } else {
        sequence!(pm, pt, {
            spt     = point;
            name    = pathed_ident;
            literal = optional(expr_value_struct_literal);
        }, |_, pt| Value { extent: ex(spt, pt), name, literal })
    }
}

fn expr_value_struct_literal<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, StructLiteral> {
    sequence!(pm, pt, {
        spt         = point;
        ws          = optional_whitespace(Vec::new());
        _           = literal("{");
        ws          = optional_whitespace(ws);
        fields      = zero_or_more_tailed_values(",", expr_value_struct_literal_field);
        (splat, ws) = concat_whitespace(ws, optional(expr_value_struct_literal_splat));
        ws          = optional_whitespace(ws);
        _           = literal("}");
    }, |_, pt| StructLiteral {
        extent: ex(spt, pt),
        fields,
        splat: splat.map(Box::new),
        whitespace: ws,
    })
}

fn expr_value_struct_literal_field<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, StructLiteralField> {
    sequence!(pm, pt, {
        spt         = point;
        name        = ident;
        mpt         = point;
        ws          = optional_whitespace(Vec::new());
        (value, ws) = concat_whitespace(ws, optional(expr_value_struct_literal_field_value));
    }, |_, _| {
        let value = value.unwrap_or_else(|| Expression::Value(Value {
            extent: ex(spt, mpt),
            name: name.into(),
            literal: None,
        }));
        StructLiteralField { name, value, whitespace: ws }
    })
}

fn expr_value_struct_literal_field_value<'s>(pm: &mut Master<'s>, pt: Point<'s>) ->
    Progress<'s, (Expression, Vec<Whitespace>)>
{
    sequence!(pm, pt, {
        ws    = optional_whitespace(Vec::new());
        _     = literal(":");
        ws    = optional_whitespace(ws);
        value = allow_struct_literals(expression);
    }, |_, _| (value, ws))
}

fn expr_value_struct_literal_splat<'s>(pm: &mut Master<'s>, pt: Point<'s>) ->
    Progress<'s, (Expression, Vec<Whitespace>)>
{
    sequence!(pm, pt, {
        ws    = optional_whitespace(Vec::new());
        _     = literal("..");
        ws    = optional_whitespace(ws);
        value = allow_struct_literals(expression);
    }, |_, _| (value, ws))
}

fn expr_disambiguation<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Disambiguation> {
    sequence!(pm, pt, {
        spt        = point;
        core       = disambiguation_core;
        components = zero_or_more_tailed_values_resume("::", path_component);
    }, move |_, pt| Disambiguation {
        extent: ex(spt, pt),
        from_type: core.from_type,
        to_type: core.to_type,
        components,
        whitespace: core.whitespace,
    })
}

fn expression_tail<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, ExpressionTail> {
    pm.alternate(pt)
        .one(expr_tail_as_type)
        .one(expr_tail_binary)
        .one(expr_tail_call)
        .one(expr_tail_field_access)
        .one(expr_tail_range)
        .one(expr_tail_slice)
        .one(expr_tail_try_operator)
        .finish()
}

fn expr_tail_as_type<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, ExpressionTail> {
    sequence!(pm, pt, {
        ws  = whitespace;
        _   = keyword("as");
        ws  = append_whitespace(ws);
        typ = typ;
    }, |_, _| ExpressionTail::AsType { typ, whitespace: ws })
}

fn expr_tail_binary<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, ExpressionTail> {
    sequence!(pm, pt, {
        ws  = optional_whitespace(Vec::new());
        op  = binary_op;
        ws  = optional_whitespace(ws);
        rhs = expression;
    }, |_, _| ExpressionTail::Binary { op, rhs: Box::new(rhs), whitespace: ws })
}

fn binary_op<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, BinaryOp> {
    // Longer operators before shorter to avoid matching += as +
    pm.alternate(pt)
        .one(map(literal("<<="), |_| BinaryOp::ShiftLeftAssign))
        .one(map(literal(">>="), |_| BinaryOp::ShiftRightAssign))
        .one(map(literal("!="), |_| BinaryOp::NotEqual))
        .one(map(literal("=="), |_| BinaryOp::Equal))
        .one(map(literal("&&"), |_| BinaryOp::BooleanAnd))
        .one(map(literal("||"), |_| BinaryOp::BooleanOr))
        .one(map(literal("+="), |_| BinaryOp::AddAssign))
        .one(map(literal("-="), |_| BinaryOp::SubAssign))
        .one(map(literal("*="), |_| BinaryOp::MulAssign))
        .one(map(literal("/="), |_| BinaryOp::DivAssign))
        .one(map(literal("%="), |_| BinaryOp::ModAssign))
        .one(map(literal("<="), |_| BinaryOp::LessThanOrEqual))
        .one(map(literal(">="), |_| BinaryOp::GreaterThanOrEqual))
        .one(map(literal("<<"), |_| BinaryOp::ShiftLeft))
        .one(map(literal(">>"), |_| BinaryOp::ShiftRight))
        .one(map(literal("&="), |_| BinaryOp::BitwiseAndAssign))
        .one(map(literal("|="), |_| BinaryOp::BitwiseOrAssign))
        .one(map(literal("^="), |_| BinaryOp::BitwiseXorAssign))
        .one(map(literal("+"), |_| BinaryOp::Add))
        .one(map(literal("-"), |_| BinaryOp::Sub))
        .one(map(literal("*"), |_| BinaryOp::Mul))
        .one(map(literal("/"), |_| BinaryOp::Div))
        .one(map(literal("%"), |_| BinaryOp::Mod))
        .one(map(literal("<"), |_| BinaryOp::LessThan))
        .one(map(literal(">"), |_| BinaryOp::GreaterThan))
        .one(map(literal("="), |_| BinaryOp::Assign))
        .one(map(literal("&"), |_| BinaryOp::BitwiseAnd))
        .one(map(literal("|"), |_| BinaryOp::BitwiseOr))
        .one(map(literal("^"), |_| BinaryOp::BitwiseXor))
        .finish()
}

fn expr_tail_call<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, ExpressionTail> {
    sequence!(pm, pt, {
        _x   = optional_whitespace(Vec::new());
        _    = literal("(");
        _x   = optional_whitespace(_x);
        args = allow_struct_literals(zero_or_more_tailed_values(",", expression));
        _x   = optional_whitespace(_x);
        _    = literal(")");
    }, |_, _| ExpressionTail::Call { args })
}

fn expr_tail_field_access<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, ExpressionTail> {
    sequence!(pm, pt, {
        _x    = optional_whitespace(Vec::new());
        _     = literal(".");
        _x    = optional_whitespace(_x);
        field = field_name;
    }, |_, _| ExpressionTail::FieldAccess { field })
}

fn field_name<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, FieldName> {
    pm.alternate(pt)
        .one(map(path_component, FieldName::Path))
        .one(map(pure_number, FieldName::Number))
        .finish()
}

fn expr_tail_range<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, ExpressionTail> {
    sequence!(pm, pt, {
        _x  = optional_whitespace(Vec::new());
        _   = literal("..");
        _x  = optional_whitespace(_x);
        rhs = optional(expression);
    }, |_, _| ExpressionTail::Range { rhs: rhs.map(Box::new) })
}

fn expr_tail_slice<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, ExpressionTail> {
    sequence!(pm, pt, {
        _     = literal("[");
        ws    = optional_whitespace(Vec::new());
        range = allow_struct_literals(expression);
        ws    = optional_whitespace(ws);
        _     = literal("]");
    }, |_, _| ExpressionTail::Slice { range: Box::new(range), whitespace: ws })
}

fn expr_tail_try_operator<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, ExpressionTail> {
    sequence!(pm, pt, {
        _x    = optional_whitespace(Vec::new());
        _     = literal("?");
    }, |_, _| ExpressionTail::TryOperator)
}

fn path<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Path> {
    sequence!(pm, pt, {
        spt        = point;
        _x         = optional_whitespace(Vec::new());
        _          = optional(literal("::"));
        _x         = optional_whitespace(_x);
        components = one_or_more_tailed_values("::", ident);
    }, |_, pt| Path { extent: ex(spt, pt), components })
}

fn pathed_ident<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, PathedIdent> {
    sequence!(pm, pt, {
        spt        = point;
        _x         = optional_whitespace(Vec::new());
        _          = optional(literal("::"));
        _x         = optional_whitespace(_x);
        components = one_or_more_tailed_values("::", path_component);
    }, |_, pt| PathedIdent { extent: ex(spt, pt), components })
}

fn path_component<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, PathComponent> {
    sequence!(pm, pt, {
        spt       = point;
        ident     = ident;
        turbofish = optional(turbofish);
    }, |_, pt| PathComponent { extent: ex(spt, pt), ident, turbofish })
}

fn turbofish<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Turbofish> {
    sequence!(pm, pt, {
        spt   = point;
        _x    = optional_whitespace(Vec::new());
        _     = literal("::");
        _x    = optional_whitespace(_x);
        _     = literal("<");
        _x    = optional_whitespace(_x);
        types = zero_or_more_tailed_values(",", typ);
        _x    = optional_whitespace(_x);
        _     = literal(">");
    }, |_, pt| Turbofish { extent: ex(spt, pt), types: types })
}

fn pattern<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Pattern> {
    sequence!(pm, pt, {
        spt  = point;
        name = optional(pattern_name);
        kind = pattern_kind;
    }, |_, pt| Pattern { extent: ex(spt, pt), name, kind })
}

fn pattern_name<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, PatternName> {
    sequence!(pm, pt, {
        spt    = point;
        is_ref = optional(pattern_ident_is_ref);
        is_mut = optional(pattern_ident_is_mut);
        name   = ident;
        ws     = optional_whitespace(Vec::new());
        _      = literal("@");
        ws     = optional_whitespace(ws);
    }, |_, _| PatternName { extent: ex(spt, pt), is_ref, is_mut, name, whitespace: ws })
}

fn pattern_kind<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, PatternKind> {
    pm.alternate(pt)
        // Must precede character and number as it contains them
        .one(map(pattern_range, PatternKind::Range))
        .one(map(pattern_char, PatternKind::Character))
        .one(map(pattern_byte, PatternKind::Byte))
        .one(map(pattern_number, PatternKind::Number))
        .one(map(pattern_reference, PatternKind::Reference))
        .one(map(pattern_byte_string, PatternKind::ByteString))
        .one(map(pattern_string, PatternKind::String))
        .one(map(pattern_struct, PatternKind::Struct))
        .one(map(pattern_tuple, PatternKind::Tuple))
        // Must be last, otherwise it collides with struct names
        .one(map(pattern_ident, PatternKind::Ident))
        .finish()
}

fn pattern_ident<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, PatternIdent> {
    sequence!(pm, pt, {
        spt    = point;
        is_ref = optional(pattern_ident_is_ref);
        is_mut = optional(pattern_ident_is_mut);
        ident  = pathed_ident;
        tuple  = optional(pattern_tuple);
    }, |_, pt| PatternIdent { extent: ex(spt, pt), is_ref, is_mut, ident, tuple })
}

fn pattern_ident_is_mut<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Extent> {
    sequence!(pm, pt, {
        mutable = ext(keyword("mut"));
        _x      = whitespace;
    }, |_, _| mutable)
}

fn pattern_ident_is_ref<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Extent> {
    sequence!(pm, pt, {
        mutable = ext(keyword("ref"));
        _x      = whitespace;
    }, |_, _| mutable)
}

fn pattern_tuple<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, PatternTuple> {
    sequence!(pm, pt, {
        spt     = point;
        _       = literal("(");
        _x      = optional_whitespace(Vec::new());
        members = zero_or_more_tailed_values(",", pattern_tuple_member);
        _x      = optional_whitespace(_x);
        _       = literal(")");
    }, |_, pt| PatternTuple { extent: ex(spt, pt), members })
}

fn pattern_tuple_member<'s>(pm: &mut Master<'s>, pt: Point<'s>) ->
    Progress<'s, PatternTupleMember>
{
    pm.alternate(pt)
        .one(map(pattern, PatternTupleMember::Pattern))
        .one(map(ext(literal("..")), PatternTupleMember::Wildcard))
        .finish()
}

fn pattern_struct<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, PatternStruct> {
    sequence!(pm, pt, {
        spt      = point;
        name     = pathed_ident;
        ws       = optional_whitespace(Vec::new());
        _        = literal("{");
        ws       = optional_whitespace(ws);
        fields   = zero_or_more_tailed_values(",", pattern_struct_field);
        ws       = optional_whitespace(ws);
        wildcard = optional(literal(".."));
        ws       = optional_whitespace(ws);
        _        = literal("}");
    }, |_, pt| PatternStruct {
        extent: ex(spt, pt),
        name,
        fields,
        wildcard: wildcard.is_some(),
        whitespace: ws,
    })
}

fn pattern_struct_field<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, PatternStructField> {
    pm.alternate(pt)
        .one(map(pattern_struct_field_long, PatternStructField::Long))
        .one(map(map(pattern_ident, |ident| {
            PatternStructFieldShort { ident }
        }), PatternStructField::Short))
        .finish()
}

fn pattern_struct_field_long<'s>(pm: &mut Master<'s>, pt: Point<'s>) ->
    Progress<'s, PatternStructFieldLong>
{
    sequence!(pm, pt, {
        spt     = point;
        name    = ident;
        ws      = optional_whitespace(Vec::new());
        _       = literal(":");
        ws      = optional_whitespace(ws);
        pattern = pattern;
    }, |_, pt| PatternStructFieldLong { extent: ex(spt, pt), name, pattern, whitespace: ws })
}

fn pattern_byte<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, PatternByte> {
    expr_byte(pm, pt).map(|value| PatternByte { extent: value.extent, value })
}

fn pattern_char<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, PatternCharacter> {
    character_literal(pm, pt).map(|value| PatternCharacter { extent: value.extent, value })
}

fn pattern_byte_string<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, PatternByteString> {
    expr_byte_string(pm, pt).map(|value| PatternByteString { extent: value.extent, value })
}

fn pattern_string<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, PatternString> {
    string_literal(pm, pt).map(|value| PatternString { extent: value.extent, value })
}

fn pattern_number<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, PatternNumber> {
    number_literal(pm, pt).map(|value| PatternNumber { extent: value.extent(), value })
}

fn pattern_reference<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, PatternReference> {
    sequence!(pm, pt, {
        spt          = point;
        _            = literal("&");
        (is_mut, ws) = concat_whitespace(Vec::new(), optional(pattern_reference_mut));
        ws           = optional_whitespace(ws);
        pattern      = pattern;
    }, |_, pt| PatternReference {
        extent: ex(spt, pt),
        is_mut,
        pattern: Box::new(pattern),
        whitespace: ws
    })
}

fn pattern_reference_mut<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, (Extent, Vec<Whitespace>)> {
    sequence!(pm, pt, {
        is_mut  = ext(keyword("mut"));
        ws      = whitespace;
    }, |_, _| (is_mut, ws))
}

fn pattern_range<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, PatternRange> {
    sequence!(pm, pt, {
        spt   = point;
        start = pattern_range_component;
        ws    = optional_whitespace(Vec::new());
        _     = literal("...");
        ws    = optional_whitespace(ws);
        end   = pattern_range_component;
    }, |_, pt| PatternRange { extent: ex(spt, pt), start, end, whitespace: ws })
}

fn pattern_range_component<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, PatternRangeComponent> {
    pm.alternate(pt)
        .one(map(character_literal, PatternRangeComponent::Character))
        .one(map(expr_byte, PatternRangeComponent::Byte))
        .one(map(number_literal, PatternRangeComponent::Number))
        .finish()
}

fn p_struct<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Struct> {
    sequence!(pm, pt, {
        spt            = point;
        visibility     = optional(visibility);
        _              = keyword("struct");
        ws             = whitespace;
        name           = ident;
        ws             = optional_whitespace(ws);
        generics       = optional(generic_declarations);
        ws             = optional_whitespace(ws);
        (body, wheres) = struct_defn_body;
    }, |_, pt| Struct {
        extent: ex(spt, pt),
        visibility,
        name,
        generics,
        wheres: wheres.unwrap_or_else(Vec::new),
        body,
        whitespace: ws,
    })
}

fn struct_defn_body<'s>(pm: &mut Master<'s>, pt: Point<'s>) ->
    Progress<'s, (StructDefinitionBody, Option<Vec<Where>>)>
{
    pm.alternate(pt)
        .one(map(struct_defn_body_brace, |(b, w)| (StructDefinitionBody::Brace(b), w)))
        .one(map(struct_defn_body_tuple, |(b, w)| (StructDefinitionBody::Tuple(b), w)))
        .one(map(ext(literal(";")), |b| (StructDefinitionBody::Empty(b), None)))
        .finish()
}

fn struct_defn_body_brace<'s>(pm: &mut Master<'s>, pt: Point<'s>) ->
    Progress<'s, (StructDefinitionBodyBrace, Option<Vec<Where>>)>
{
    sequence!(pm, pt, {
        _x           = optional_whitespace(Vec::new());
        (wheres, _x) = concat_whitespace(_x, optional(where_clause));
        _x           = optional_whitespace(_x);
        body         = struct_defn_body_brace_only;
    }, |_, _| (body, wheres))
}

fn struct_defn_body_brace_only<'s>(pm: &mut Master<'s>, pt: Point<'s>) ->
    Progress<'s, StructDefinitionBodyBrace>
{
    sequence!(pm, pt, {
        spt          = point;
        _            = literal("{");
        ws           = optional_whitespace(Vec::new());
        fields       = zero_or_more_tailed_values(",", struct_defn_field);
        ws           = optional_whitespace(ws);
        _            = literal("}");
    }, |_, pt| StructDefinitionBodyBrace { extent: ex(spt, pt), fields, whitespace: ws })
}

fn struct_defn_body_tuple<'s>(pm: &mut Master<'s>, pt: Point<'s>) ->
    Progress<'s, (StructDefinitionBodyTuple, Option<Vec<Where>>)>
{
    sequence!(pm, pt, {
        spt          = point;
        fields       = struct_defn_body_tuple_only;
        ws           = optional_whitespace(Vec::new());
        (wheres, ws) = concat_whitespace(ws, optional(where_clause));
        ws           = optional_whitespace(ws);
        _            = literal(";");
    }, |_, pt| (StructDefinitionBodyTuple { extent: ex(spt, pt), fields, whitespace: ws }, wheres))
}

fn struct_defn_body_tuple_only<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Vec<StructDefinitionFieldUnnamed>> {
    sequence!(pm, pt, {
        _     = literal("(");
        _x    = optional_whitespace(Vec::new());
        types = zero_or_more_tailed_values(",", tuple_defn_field);
        _x    = optional_whitespace(_x);
        _     = literal(")");
    }, |_, _| types)
}

fn tuple_defn_field<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, StructDefinitionFieldUnnamed> {
    sequence!(pm, pt, {
        spt        = point;
        attributes = zero_or_more(struct_defn_field_attr);
        visibility = optional(visibility);
        typ        = typ;
    }, |_, pt| StructDefinitionFieldUnnamed {
        extent: ex(spt, pt),
        attributes,
        visibility,
        typ,
    })
}

fn struct_defn_field<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, StructDefinitionFieldNamed> {
    sequence!(pm, pt, {
        spt        = point;
        attributes = zero_or_more(struct_defn_field_attr);
        visibility = optional(visibility);
        name       = ident;
        ws         = optional_whitespace(Vec::new());
        _          = literal(":");
        ws         = optional_whitespace(ws);
        typ        = typ;
    }, |_, pt| StructDefinitionFieldNamed {
        extent: ex(spt, pt),
        visibility,
        attributes,
        name,
        typ,
        whitespace: ws,
    })
}

fn struct_defn_field_attr<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Attribute> {
    sequence!(pm, pt, {
        attribute = attribute;
        _x = optional(whitespace);
    }, |_, _| attribute)
}

fn p_enum<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Enum> {
    sequence!(pm, pt, {
        spt          = point;
        visibility   = optional(visibility);
        _            = keyword("enum");
        ws           = whitespace;
        name         = ident;
        ws           = optional_whitespace(ws);
        generics     = optional(generic_declarations);
        ws           = optional_whitespace(ws);
        (wheres, ws) = concat_whitespace(ws, optional(where_clause));
        ws           = optional_whitespace(ws);
        _            = literal("{");
        ws           = optional_whitespace(ws);
        variants     = zero_or_more_tailed_values(",", enum_variant);
        ws           = optional_whitespace(ws);
        _            = literal("}");
    }, |_, pt| Enum {
        extent: ex(spt, pt),
        visibility,
        name,
        generics,
        wheres: wheres.unwrap_or_else(Vec::new),
        variants,
        whitespace: ws,
    })
}

fn enum_variant<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, EnumVariant> {
    sequence!(pm, pt, {
        spt  = point;
        attributes = zero_or_more(struct_defn_field_attr);
        name = ident;
        ws   = optional_whitespace(Vec::new());
        body = enum_variant_body;
    }, |_, pt| EnumVariant { extent: ex(spt, pt), attributes, name, body, whitespace: ws })
}

fn enum_variant_body<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, EnumVariantBody> {
    pm.alternate(pt)
        .one(map(struct_defn_body_tuple_only, EnumVariantBody::Tuple))
        .one(map(struct_defn_body_brace_only, EnumVariantBody::Struct))
        .one(map(optional(enum_discriminant), EnumVariantBody::Unit))
        .finish()
}

fn enum_discriminant<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Expression> {
    sequence!(pm, pt, {
        _     = literal("=");
        _x    = optional_whitespace(Vec::new());
        value = expression;
    }, |_, _| value)
}

fn p_trait<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Trait> {
    sequence!(pm, pt, {
        spt          = point;
        visibility   = optional(visibility);
        is_unsafe    = optional(p_trait_unsafe);
        _            = keyword("trait");
        ws           = whitespace;
        name         = ident;
        ws           = optional_whitespace(ws);
        generics     = optional(generic_declarations);
        ws           = optional_whitespace(ws);
        bounds       = optional(generic_declaration_type_bounds);
        ws           = optional_whitespace(ws);
        (wheres, ws) = concat_whitespace(ws, optional(where_clause));
        ws           = optional_whitespace(ws);
        _            = literal("{");
        ws           = optional_whitespace(ws);
        members      = zero_or_more(trait_impl_member);
        ws           = optional_whitespace(ws);
        _            = literal("}");
    }, |_, pt| Trait {
        extent: ex(spt, pt),
        visibility,
        is_unsafe,
        name,
        generics,
        bounds,
        wheres: wheres.unwrap_or_else(Vec::new),
        members,
        whitespace: ws,
    })
}

fn p_trait_unsafe<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Extent> {
    sequence!(pm, pt, {
        ex = ext(keyword("unsafe"));
        _x = whitespace;
    }, |_, _| ex)
}

// TOOD: this is a terrrrrrible name. It is not an impl!
fn trait_impl_member<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TraitMember> {
    pm.alternate(pt)
        .one(map(trait_member_function, TraitMember::Function))
        .one(map(trait_member_type, TraitMember::Type))
        .one(map(attribute, TraitMember::Attribute))
        .one(map(whitespace, TraitMember::Whitespace))
        .finish()
}

fn trait_member_function<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TraitMemberFunction> {
    sequence!(pm, pt, {
        spt    = point;
        header = trait_impl_function_header;
        _x     = optional_whitespace(Vec::new());
        body   = trait_impl_function_body;
    }, |_, pt| TraitMemberFunction { extent: ex(spt, pt), header, body })
}

fn trait_member_type<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TraitMemberType> {
    sequence!(pm, pt, {
        spt    = point;
        _      = keyword("type");
        ws     = whitespace;
        name   = ident;
        bounds = optional(generic_declaration_type_bounds);
        ws     = optional_whitespace(ws);
        _      = literal(";");
    }, |_, pt| TraitMemberType { extent: ex(spt, pt), name, bounds, whitespace: ws })
}

fn visibility<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Visibility> {
    sequence!(pm, pt, {
        spt       = point;
        _         = keyword("pub");
        ws        = optional_whitespace(Vec::new());
        qualifier = optional(visibility_qualifier);
        ws        = optional_whitespace(ws);
    }, |_, pt| Visibility { extent: ex(spt, pt), qualifier, whitespace: ws })
}

fn visibility_qualifier<'s>(pm: &mut Master<'s>, pt: Point<'s>) ->
    Progress<'s, VisibilityQualifier>
{
    sequence!(pm, pt, {
        _         = literal("(");
        _x        = optional_whitespace(Vec::new());
        qualifier = visibility_qualifier_kind;
        _x        = optional_whitespace(_x);
        _         = literal(")");
    }, |_, _| qualifier)
}

fn visibility_qualifier_kind<'s>(pm: &mut Master<'s>, pt: Point<'s>) ->
    Progress<'s, VisibilityQualifier>
{
    pm.alternate(pt)
        .one(map(keyword("crate"), |_| VisibilityQualifier::Crate))
        .one(map(path, VisibilityQualifier::Path))
        .finish()
}

// TODO: Massively duplicated!!!
fn trait_impl_function_header<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TraitImplFunctionHeader> {
    sequence!(pm, pt, {
        spt               = point;
        visibility        = optional(visibility);
        qualifiers        = function_qualifiers;
        _                 = keyword("fn");
        ws                = whitespace;
        name              = ident;
        generics          = optional(generic_declarations);
        ws                = optional_whitespace(ws);
        arguments         = trait_impl_function_arglist;
        ws                = optional_whitespace(ws);
        (return_type, ws) = concat_whitespace(ws, optional(function_return_type));
        ws                = optional_whitespace(ws);
        (wheres, ws)      = concat_whitespace(ws, optional(where_clause));
    }, |_, pt| {
        TraitImplFunctionHeader {
            extent: ex(spt, pt),
            visibility,
            qualifiers,
            name,
            generics,
            arguments,
            return_type,
            wheres: wheres.unwrap_or_else(Vec::new),
            whitespace: ws,
        }})
}

fn trait_impl_function_arglist<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Vec<TraitImplArgument>> {
    sequence!(pm, pt, {
        _        = literal("(");
        _x       = optional_whitespace(Vec::new());
        self_arg = optional(map(self_argument, TraitImplArgument::SelfArgument));
        args     = zero_or_more_tailed_values_append(self_arg, ",", trait_impl_function_argument);
        _x       = optional_whitespace(_x);
        _        = literal(")");
    }, move |_, _| args)
}

fn trait_impl_function_argument<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TraitImplArgument> {
    sequence!(pm, pt, {
        (name, ws) = concat_whitespace(Vec::new(), optional(trait_impl_function_argument_name));
        typ        = typ;
    }, |_, _| TraitImplArgument::Named(TraitImplArgumentNamed { name, typ, whitespace: ws }))
}

fn trait_impl_function_argument_name<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, (Pattern, Vec<Whitespace>)> {
    sequence!(pm, pt, {
        name = pattern;
        ws   = optional_whitespace(Vec::new());
        _    = literal(":");
        ws   = optional_whitespace(ws);
    }, |_, _| (name, ws))
}

fn trait_impl_function_body<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Option<Block>> {
    pm.alternate(pt)
        .one(map(block, Some))
        .one(map(literal(";"), |_| None))
        .finish()
}

fn p_impl<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Impl> {
    sequence!(pm, pt, {
        spt              = point;
        (is_unsafe, ws)  = concat_whitespace(Vec::new(), optional(p_impl_unsafe));
        _                = keyword("impl");
        generics         = optional(generic_declarations);
        ws               = append_whitespace(ws);
        of_trait         = optional(p_impl_of_trait);
        type_name        = typ;
        ws               = optional_whitespace(ws);
        (wheres, ws)     = concat_whitespace(ws, optional(where_clause));
        ws               = optional_whitespace(ws);
        _                = literal("{");
        ws               = optional_whitespace(ws);
        body             = zero_or_more(impl_member);
        ws               = optional_whitespace(ws);
        _                = literal("}");
    }, |_, pt| Impl {
        extent: ex(spt, pt),
        is_unsafe,
        generics,
        of_trait,
        type_name,
        wheres: wheres.unwrap_or_else(Vec::new),
        body,
        whitespace: ws,
    })
}

fn p_impl_unsafe<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, (Extent, Vec<Whitespace>)> {
    sequence!(pm, pt, {
        us = ext(keyword("unsafe"));
        ws = whitespace;
    }, |_, _| (us, ws))
}

fn p_impl_of_trait<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, ImplOfTrait> {
    sequence!(pm, pt, {
        spt         = point;
        is_negative = optional(ext(literal("!")));
        ws          = optional_whitespace(Vec::new());
        trait_name  = typ;
        ws          = append_whitespace(ws);
        _           = keyword("for");
        ws          = append_whitespace(ws);
    }, |_, pt| ImplOfTrait { extent: ex(spt, pt), is_negative, trait_name, whitespace: ws })
}

fn impl_member<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, ImplMember> {
    pm.alternate(pt)
        .one(map(attribute, ImplMember::Attribute))
        .one(map(impl_function, ImplMember::Function))
        .one(map(impl_type, ImplMember::Type))
        .one(map(item_macro_call, ImplMember::MacroCall))
        .one(map(whitespace, ImplMember::Whitespace))
        .finish()
}

fn impl_function<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, ImplFunction> {
    sequence!(pm, pt, {
        spt    = point;
        header = function_header;
        _x     = optional_whitespace(Vec::new());
        body   = block;
    }, |_, pt| ImplFunction { extent: ex(spt, pt), header, body })
}

fn impl_type<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, ImplType> {
    sequence!(pm, pt, {
        spt  = point;
        _    = keyword("type");
        ws   = whitespace;
        name = ident;
        ws   = optional_whitespace(ws);
        _    = literal("=");
        ws   = optional_whitespace(ws);
        typ  = typ;
        ws   = optional_whitespace(ws);
        _    = literal(";");
    }, |_, pt| ImplType { extent: ex(spt, pt), name, typ, whitespace: ws })
}

// TODO: optional could take E that is `into`, or just a different one

fn attribute<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Attribute> {
    sequence!(pm, pt, {
        spt           = point;
        _             = literal("#");
        is_containing = optional(ext(literal("!")));
        _             = literal("[");
        text          = parse_nested_tokens_until(Token::is_left_square, Token::is_right_square);
        _             = literal("]");
    }, |_, pt| Attribute { extent: ex(spt, pt), is_containing, text })
}

fn p_const<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Const> {
    sequence!(pm, pt, {
        spt        = point;
        visibility = optional(visibility);
        _          = keyword("const");
        ws         = whitespace;
        name       = ident;
        ws         = optional_whitespace(ws);
        _          = literal(":");
        ws         = optional_whitespace(ws);
        typ        = typ;
        ws         = optional_whitespace(ws);
        _          = literal("=");
        ws         = optional_whitespace(ws);
        value      = expression;
        _          = literal(";");
    }, |_, pt| Const { extent: ex(spt, pt), visibility, name, typ, value, whitespace: ws })
}

fn p_static<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Static> {
    sequence!(pm, pt, {
        spt        = point;
        visibility = optional(visibility);
        _          = keyword("static");
        ws         = whitespace;
        is_mut     = optional(ext(keyword("mut")));
        ws         = optional_whitespace(ws);
        name       = ident;
        ws         = optional_whitespace(ws);
        _          = literal(":");
        ws         = optional_whitespace(ws);
        typ        = typ;
        ws         = optional_whitespace(ws);
        _          = literal("=");
        ws         = optional_whitespace(ws);
        value      = expression;
        _          = literal(";");
    }, |_, pt| Static { extent: ex(spt, pt), visibility, is_mut, name, typ, value, whitespace: ws })
}

fn extern_crate<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Crate> {
    sequence!(pm, pt, {
        spt        = point;
        visibility = optional(visibility);
        _          = keyword("extern");
        ws         = whitespace;
        _          = keyword("crate");
        ws         = append_whitespace(ws);
        name       = ident;
        rename     = optional(extern_crate_rename);
        ws         = optional_whitespace(ws);
        _          = literal(";");
    }, |_, pt| Crate { extent: ex(spt, pt), visibility, name, rename, whitespace: ws })
}

fn extern_crate_rename<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Ident> {
    sequence!(pm, pt, {
        _x   = whitespace;
        _    = keyword("as");
        _x   = whitespace;
        name = ident;
    }, |_, _| name)
}

fn extern_block<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, ExternBlock> {
    sequence!(pm, pt, {
        spt     = point;
        _       = keyword("extern");
        abi     = optional(extern_block_abi);
        ws      = optional_whitespace(Vec::new());
        _       = literal("{");
        members = zero_or_more(extern_block_member);
        _       = literal("}");
    }, |_, pt| ExternBlock { extent: ex(spt, pt), abi, members, whitespace: ws })
}

fn extern_block_abi<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, String> {
    sequence!(pm, pt, {
        _x  = whitespace;
        abi = string_literal;
    }, |_, _| abi)
}

fn extern_block_member<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, ExternBlockMember> {
    pm.alternate(pt)
        .one(map(attribute, ExternBlockMember::Attribute))
        .one(map(extern_block_member_function, ExternBlockMember::Function))
        .one(map(whitespace, ExternBlockMember::Whitespace))
        .finish()
}

// TODO: Massively duplicated!!!
fn extern_block_member_function<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, ExternBlockMemberFunction> {
    sequence!(pm, pt, {
        spt    = point;
        visibility        = optional(visibility);
        _                 = keyword("fn");
        ws                = whitespace;
        name              = ident;
        ws                = optional_whitespace(ws);
        arguments         = extern_block_function_arglist;
        ws                = optional_whitespace(ws);
        (return_type, ws) = concat_whitespace(ws, optional(function_return_type));
        ws                = optional_whitespace(ws);
        (wheres, ws)      = concat_whitespace(ws, optional(where_clause));
        _                 = literal(";");
    }, |_, pt| {
        ExternBlockMemberFunction {
            extent: ex(spt, pt),
            visibility,
            name,
            arguments,
            return_type,
            wheres: wheres.unwrap_or_else(Vec::new),
            whitespace: ws,
        }
    })
}

fn extern_block_function_arglist<'s>(pm: &mut Master<'s>, pt: Point<'s>) ->
    Progress<'s, Vec<ExternBlockMemberFunctionArgument>>
{
    sequence!(pm, pt, {
        _    = literal("(");
        _x   = optional_whitespace(Vec::new());
        args = zero_or_more_tailed_values(",", extern_block_function_argument);
        _x   = optional_whitespace(_x);
        _    = literal(")");
    }, move |_, _| args)
}

fn extern_block_function_argument<'s>(pm: &mut Master<'s>, pt: Point<'s>) ->
    Progress<'s, ExternBlockMemberFunctionArgument>
{
    pm.alternate(pt)
        .one(map(extern_block_function_argument_named, ExternBlockMemberFunctionArgument::Named))
        .one(map(extern_block_function_argument_variadic, ExternBlockMemberFunctionArgument::Variadic))
        .finish()
}

fn extern_block_function_argument_named<'s>(pm: &mut Master<'s>, pt: Point<'s>) ->
    Progress<'s, ExternBlockMemberFunctionArgumentNamed>
{
    sequence!(pm, pt, {
        spt  = point;
        name = pattern;
        ws   = optional_whitespace(Vec::new());
        _    = literal(":");
        ws   = optional_whitespace(ws);
        typ  = typ;
    }, |_, pt| ExternBlockMemberFunctionArgumentNamed {
        extent: ex(spt, pt),
        name,
        typ,
        whitespace: ws,
    })
}

fn extern_block_function_argument_variadic<'s>(pm: &mut Master<'s>, pt: Point<'s>) ->
    Progress<'s, ExternBlockMemberFunctionArgumentVariadic>
{
    sequence!(pm, pt, {
        spt  = point;
        _    = literal("...");
    }, |_, pt| ExternBlockMemberFunctionArgumentVariadic { extent: ex(spt, pt) })
}

fn p_use<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Use> {
    sequence!(pm, pt, {
        spt        = point;
        visibility = optional(visibility);
        _          = keyword("use");
        ws         = whitespace;
        _          = optional(literal("::"));
        ws         = optional_whitespace(ws);
        path       = zero_or_more(use_path_component);
        ws         = optional_whitespace(ws);
        tail       = use_tail;
        ws         = optional_whitespace(ws);
        _          = literal(";");
    }, move |_, pt| {
        Use { extent: ex(spt, pt), visibility, path, tail, whitespace: ws }
    })
}

fn use_path_component<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Ident> {
    sequence!(pm, pt, {
        name = ident;
        _x   = optional_whitespace(Vec::new());
        _    = literal("::");
    }, |_, _| name)
}

fn use_tail<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, UseTail> {
    pm.alternate(pt)
        .one(map(use_tail_ident, UseTail::Ident))
        .one(map(use_tail_glob, UseTail::Glob))
        .one(map(use_tail_multi, UseTail::Multi))
        .finish()
}

fn use_tail_ident<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, UseTailIdent> {
    sequence!(pm, pt, {
        spt = point;
        name = ident;
        rename = optional(use_tail_ident_rename);
    }, |_, pt| UseTailIdent { extent: ex(spt, pt), name, rename })
}

fn use_tail_ident_rename<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Ident> {
    sequence!(pm, pt, {
        _x   = whitespace;
        _    = keyword("as");
        _x   = append_whitespace(_x);
        name = ident;
    }, |_, _| name)
}

fn use_tail_glob<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, UseTailGlob> {
    sequence!(pm, pt, {
        spt = point;
        _   = literal("*");
    }, |_, pt| UseTailGlob { extent: ex(spt, pt) })
}

fn use_tail_multi<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, UseTailMulti> {
    sequence!(pm, pt, {
        spt   = point;
        _     = literal("{");
        _x    = optional_whitespace(Vec::new());
        names = zero_or_more_tailed_values(",", use_tail_ident);
        _x    = optional_whitespace(_x);
        _     = literal("}");
    }, |_, pt| UseTailMulti { extent: ex(spt, pt), names })
}

fn type_alias<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TypeAlias> {
    sequence!(pm, pt, {
        spt        = point;
        visibility = optional(visibility);
        _          = keyword("type");
        ws         = whitespace;
        name       = typ;
        ws         = optional_whitespace(ws);
        _          = literal("=");
        ws         = optional_whitespace(ws);
        defn       = typ;
        ws         = optional_whitespace(ws);
        _          = literal(";");
    }, |_, pt| TypeAlias { extent: ex(spt, pt), visibility, name, defn, whitespace: ws })
}

fn module<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Module> {
    sequence!(pm, pt, {
        spt        = point;
        visibility = optional(visibility);
        _          = keyword("mod");
        ws         = whitespace;
        name       = ident;
        ws         = optional_whitespace(ws);
        body       = module_body_or_not;
    }, |_, pt| Module { extent: ex(spt, pt), visibility, name, body, whitespace: ws })
}

fn module_body_or_not<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Option<Vec<Item>>> {
    pm.alternate(pt)
        .one(map(module_body, Some))
        .one(map(literal(";"), |_| None))
        .finish()
}

fn module_body<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Vec<Item>> {
    sequence!(pm, pt, {
        _    = literal("{");
        body = zero_or_more(item);
        _    = literal("}");
    }, |_, _| body)
}

fn typ<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Type> {
    pm.alternate(pt)
        .one(map(typ_array, Type::Array))
        .one(map(typ_combination, Type::Combination))
        .one(map(typ_function, Type::Function))
        .one(map(typ_pointer, Type::Pointer))
        .one(map(typ_reference, Type::Reference))
        .one(map(typ_slice, Type::Slice))
        .one(map(typ_tuple, Type::Tuple))
        .one(map(ext(literal("!")), Type::Uninhabited))
        .finish()
}

fn typ_reference<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TypeReference> {
    sequence!(pm, pt, {
        spt  = point;
        kind = typ_reference_kind;
        typ  = typ;
    }, |_, pt| TypeReference { extent: ex(spt, pt), kind, typ: Box::new(typ) })
}

fn typ_reference_kind<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TypeReferenceKind> {
    sequence!(pm, pt, {
        spt      = point;
        _        = literal("&");
        ws       = optional_whitespace(Vec::new());
        lifetime = optional(lifetime);
        ws       = optional_whitespace(ws);
        mutable  = optional(ext(keyword("mut")));
        ws       = optional_whitespace(ws);
    }, |_, pt| TypeReferenceKind { extent: ex(spt, pt), lifetime, mutable, whitespace: ws })
}

fn typ_pointer<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TypePointer> {
    sequence!(pm, pt, {
        spt  = point;
        _    = literal("*");
        ws   = optional_whitespace(Vec::new());
        kind = typ_pointer_kind;
        ws   = append_whitespace(ws);
        typ  = typ;
    }, |_, pt| TypePointer { extent: ex(spt, pt), kind, typ: Box::new(typ), whitespace: ws })
}

fn typ_pointer_kind<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TypePointerKind> {
    pm.alternate(pt)
        .one(map(keyword("const"), |_| TypePointerKind::Const))
        .one(map(keyword("mut"), |_| TypePointerKind::Mutable))
        .finish()
}

fn typ_tuple<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TypeTuple> {
    sequence!(pm, pt, {
        spt   = point;
        _     = literal("(");
        _x    = optional_whitespace(Vec::new());
        types = zero_or_more_tailed_values(",", typ);
        _x    = optional_whitespace(_x);
        _     = literal(")");
    }, |_, pt| TypeTuple { extent: ex(spt, pt), types })
}

fn typ_higher_ranked_trait_bounds<'s>(pm: &mut Master<'s>, pt: Point<'s>) ->
    Progress<'s, TypeHigherRankedTraitBounds>
{
    sequence!(pm, pt, {
        spt       = point;
        _         = keyword("for");
        ws        = optional_whitespace(Vec::new());
        _         = literal("<");
        ws        = optional_whitespace(ws);
        lifetimes = zero_or_more_tailed_values(",", lifetime);
        ws        = optional_whitespace(ws);
        _         = literal(">");
        ws        = optional_whitespace(ws);
        child     = typ_higher_ranked_trait_bounds_child;
    }, |_, pt| TypeHigherRankedTraitBounds { extent: ex(spt, pt), lifetimes, child, whitespace: ws })
}

fn typ_higher_ranked_trait_bounds_child<'s>(pm: &mut Master<'s>, pt: Point<'s>) ->
    Progress<'s, TypeHigherRankedTraitBoundsChild>
{
    pm.alternate(pt)
        .one(map(typ_named, TypeHigherRankedTraitBoundsChild::Named))
        .one(map(typ_function, TypeHigherRankedTraitBoundsChild::Function))
        .one(map(typ_reference, TypeHigherRankedTraitBoundsChild::Reference))
        .finish()
}

fn typ_impl_trait<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TypeImplTrait> {
    sequence!(pm, pt, {
        spt  = point;
        _    = keyword("impl");
        ws   = whitespace;
        name = typ_named;
    }, |_, pt| TypeImplTrait { extent: ex(spt, pt), name, whitespace: ws })
}

fn typ_combination<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TypeCombination> {
    sequence!(pm, pt, {
        spt        = point;
        base       = typ_combination_base;
        additional = zero_or_more_tailed_values_resume("+", typ_combination_additional);
    }, move |_, pt| TypeCombination { extent: ex(spt, pt), base, additional })
}

fn typ_combination_base<'s>(pm: &mut Master<'s>, pt: Point<'s>) ->
    Progress<'s, TypeCombinationBase>
{
    pm.alternate(pt)
        .one(map(typ_disambiguation, TypeCombinationBase::Disambiguation))
        .one(map(typ_named, TypeCombinationBase::Named))
        .one(map(typ_higher_ranked_trait_bounds, TypeCombinationBase::HigherRankedTraitBounds))
        .one(map(typ_impl_trait, TypeCombinationBase::ImplTrait))
        .finish()
}

fn typ_combination_additional<'s>(pm: &mut Master<'s>, pt: Point<'s>) ->
    Progress<'s, TypeCombinationAdditional>
{
    pm.alternate(pt)
        .one(map(typ_named, TypeCombinationAdditional::Named))
        .one(map(lifetime, TypeCombinationAdditional::Lifetime))
        .finish()
}

fn typ_named<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TypeNamed> {
    sequence!(pm, pt, {
        spt  = point;
        _    = optional(literal("::"));
        _x   = optional_whitespace(Vec::new());
        path = one_or_more_tailed_values("::", typ_named_component);
    }, |_, pt| TypeNamed { extent: ex(spt, pt), path })
}

fn typ_named_component<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TypeNamedComponent> {
    sequence!(pm, pt, {
        spt      = point;
        ident    = ident;
        generics = optional(typ_generics);
    }, |_, pt| TypeNamedComponent { extent: ex(spt, pt), ident, generics })
}

fn typ_disambiguation<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TypeDisambiguation> {
    sequence!(pm, pt, {
        spt  = point;
        core = disambiguation_core;
        path = zero_or_more_tailed_values_resume("::", typ_named_component);
    }, move |_, pt| TypeDisambiguation {
        extent: ex(spt, pt),
        from_type: Box::new(core.from_type),
        to_type: core.to_type.map(Box::new),
        path,
        whitespace: core.whitespace,
    })
}

struct DisambiguationCore {
    from_type: Type,
    to_type: Option<TypeNamed>,
    whitespace: Vec<Whitespace>,
}

fn disambiguation_core<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, DisambiguationCore> {
    sequence!(pm, pt, {
        _             = literal("<");
        ws            = optional_whitespace(Vec::new());
        from_type     = typ;
        (to_type, ws) = concat_whitespace(ws, optional(disambiguation_core_to_type));
        ws            = optional_whitespace(ws);
        _             = literal(">");
    }, |_, _| DisambiguationCore { from_type, to_type, whitespace: ws })
}

fn disambiguation_core_to_type<'s>(pm: &mut Master<'s>, pt: Point<'s>) ->
    Progress<'s, (TypeNamed, Vec<Whitespace>)>
{
    sequence!(pm, pt, {
        ws        = whitespace;
        _         = keyword("as");
        ws        = append_whitespace(ws);
        to_type   = typ_named;
    }, |_, _| (to_type, ws))
}

fn typ_array<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TypeArray> {
    sequence!(pm, pt, {
        spt   = point;
        _     = literal("[");
        ws    = optional_whitespace(Vec::new());
        typ   = typ;
        ws    = optional_whitespace(ws);
        _     = literal(";");
        ws    = optional_whitespace(ws);
        count = expression;
        ws    = optional_whitespace(ws);
        _     = literal("]");
    }, |_, pt| TypeArray {
        extent: ex(spt, pt),
        typ: Box::new(typ),
        count: Box::new(count),
        whitespace: ws,
    })
}

fn typ_slice<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TypeSlice> {
    sequence!(pm, pt, {
        spt = point;
        _   = literal("[");
        ws  = optional_whitespace(Vec::new());
        typ = typ;
        ws  = optional_whitespace(ws);
        _   = literal("]");
    }, |_, pt| TypeSlice { extent: ex(spt, pt), typ: Box::new(typ), whitespace: ws })
}

fn typ_generics<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TypeGenerics> {
    pm.alternate(pt)
        .one(map(typ_generics_fn, TypeGenerics::Function))
        .one(map(typ_generics_angle, TypeGenerics::Angle))
        .finish()
}

fn typ_generics_fn<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TypeGenericsFunction> {
    sequence!(pm, pt, {
        spt               = point;
        _                 = literal("(");
        _x                = optional_whitespace(Vec::new());
        types             = zero_or_more_tailed_values(",", typ);
        _x                = optional_whitespace(_x);
        _                 = literal(")");
        ws                = optional_whitespace(Vec::new());
        (return_type, ws) = concat_whitespace(ws, optional(function_return_type));
    }, |_, pt| TypeGenericsFunction {
        extent: ex(spt, pt),
        types,
        return_type: return_type.map(Box::new),
        whitespace: ws,
    })
}

fn typ_generics_angle<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TypeGenericsAngle> {
    sequence!(pm, pt, {
        spt     = point;
        ws      = optional_whitespace(Vec::new());
        _       = literal("<");
        ws      = optional_whitespace(ws);
        members = zero_or_more_tailed_values(",", typ_generics_angle_member);
        ws      = optional_whitespace(ws);
        _       = literal(">");
    }, |_, pt| TypeGenericsAngle { extent: ex(spt, pt), members, whitespace: ws })
}

// Parsing all of these equally is a bit inconsistent with the
// compler. The compiler *parses* lifetimes after types, but later
// errors about it. It does *not* parse associated types before types
// though.
fn typ_generics_angle_member<'s>(pm: &mut Master<'s>, pt: Point<'s>) ->
    Progress<'s, TypeGenericsAngleMember>
{
    pm.alternate(pt)
        .one(map(associated_type, TypeGenericsAngleMember::AssociatedType))
        .one(map(lifetime, TypeGenericsAngleMember::Lifetime))
        .one(map(typ, TypeGenericsAngleMember::Type))
        .finish()
}

fn associated_type<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, AssociatedType> {
    sequence!(pm, pt, {
        spt   = point;
        name  = ident;
        ws    = optional_whitespace(Vec::new());
        _     = literal("=");
        ws    = optional_whitespace(ws);
        value = typ;
    }, |_, pt| AssociatedType { extent: ex(spt, pt), name, value, whitespace: ws })
}

fn typ_function<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, TypeFunction> {
    sequence!(pm, pt, {
        spt               = point;
        qualifiers        = function_qualifiers;
        _                 = keyword("fn");
        ws                = optional_whitespace(Vec::new());
        arguments         = trait_impl_function_arglist; // TODO: shouldn't allow `self`
        ws                = optional_whitespace(ws);
        (return_type, ws) = concat_whitespace(ws, optional(function_return_type));
    }, |_, pt| TypeFunction {
        extent: ex(spt, pt),
        qualifiers,
        arguments,
        return_type: return_type.map(Box::new),
        whitespace: ws,
    })
}

fn lifetime<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Lifetime> {
    sequence!(pm, pt, {
        spt  = point;
        _    = literal("'");
        ws   = optional_whitespace(Vec::new());
        name = ident_or_static;
    }, |_, pt| Lifetime { extent: ex(spt, pt), name, whitespace: ws })
}

// TODO: Should this be an enum?
fn ident_or_static<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Ident> {
    pm.alternate(pt)
        .one(map(ext(keyword("static")), |extent| Ident { extent }))
        .one(ident)
        .finish()
}

fn whitespace<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Vec<Whitespace>> {
    one_or_more(whitespace_core)(pm, pt)
}

fn whitespace_core<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Whitespace> {
    pm.alternate(pt)
        .one(map(comment, Whitespace::Comment))
        .one(map(true_whitespace, Whitespace::Whitespace))
        .finish()
}

fn true_whitespace<'s>(pm: &mut Master<'s>, pt: Point<'s>) -> Progress<'s, Extent> {
    let spt = pt;

    let (pt, _) = try_parse!(one_or_more(|pm, pt| {
        pm.alternate(pt)
            .one(literal(" "))
            .one(literal("\t"))
            .one(literal("\r"))
            .one(literal("\n"))
            .finish()
    })(pm, pt));

    Progress::success(pt, ex(spt, pt))
}

#[cfg(test)]
mod test {
    use super::*;

    fn qp<'s, F, T>(f: F, s: &'s str) -> peresil::Progress<Point<'s>, T, Vec<Error>>
        where F: FnOnce(&mut Master<'s>, Point<'s>) -> Progress<'s, T>
    {
        // TODO: Master::once()?
        let mut pm = Master::with_state(State::default());
        let pt = Point::new(s);
        let r = f(&mut pm, pt);
        pm.finish(r)
    }

    #[test]
    fn parse_use() {
        let p = qp(p_use, "use foo::Bar;");
        assert_eq!(unwrap_progress(p).extent, (0, 13))
    }

    #[test]
    fn parse_use_public() {
        let p = qp(p_use, "pub use foo::Bar;");
        assert_eq!(unwrap_progress(p).extent, (0, 17))
    }

    #[test]
    fn parse_use_glob() {
        let p = qp(p_use, "use foo::*;");
        assert_eq!(unwrap_progress(p).extent, (0, 11))
    }

    #[test]
    fn parse_use_with_multi() {
        let p = qp(p_use, "use foo::{Bar, Baz};");
        assert_eq!(unwrap_progress(p).extent, (0, 20))
    }

    #[test]
    fn parse_use_no_path() {
        let p = qp(p_use, "use {Bar, Baz};");
        assert_eq!(unwrap_progress(p).extent, (0, 15))
    }

    #[test]
    fn parse_use_absolute_path() {
        let p = qp(p_use, "use ::{Bar, Baz};");
        assert_eq!(unwrap_progress(p).extent, (0, 17))
    }

    #[test]
    fn parse_use_rename() {
        let p = qp(p_use, "use foo as bar;");
        assert_eq!(unwrap_progress(p).extent, (0, 15))
    }

    #[test]
    fn parse_use_with_multi_rename() {
        let p = qp(p_use, "use foo::{bar as a, baz as b};");
        assert_eq!(unwrap_progress(p).extent, (0, 30))
    }

    #[test]
    fn parse_use_all_space() {
        let p = qp(p_use, "use foo :: { bar as a , baz as b } ;");
        assert_eq!(unwrap_progress(p).extent, (0, 36))
    }

    #[test]
    fn item_mod_multiple() {
        let p = qp(item, "mod foo { use super::*; }");
        assert_eq!(unwrap_progress(p).extent(), (0, 25))
    }

    #[test]
    fn item_macro_call_with_parens() {
        let p = qp(item, "foo!();");
        assert_eq!(unwrap_progress(p).extent(), (0, 7))
    }

    #[test]
    fn item_macro_call_with_square_brackets() {
        let p = qp(item, "foo![];");
        assert_eq!(unwrap_progress(p).extent(), (0, 7))
    }

    #[test]
    fn item_macro_call_with_curly_braces() {
        let p = qp(item, "foo! { }");
        assert_eq!(unwrap_progress(p).extent(), (0, 8))
    }

    #[test]
    fn item_macro_call_with_ident() {
        let p = qp(item, "macro_rules! name { }");
        assert_eq!(unwrap_progress(p).extent(), (0, 21))
    }

    #[test]
    fn item_mod() {
        let p = qp(module, "mod foo { }");
        assert_eq!(unwrap_progress(p).extent, (0, 11))
    }

    #[test]
    fn item_mod_public() {
        let p = qp(module, "pub mod foo;");
        assert_eq!(unwrap_progress(p).extent, (0, 12))
    }

    #[test]
    fn item_mod_another_file() {
        let p = qp(module, "mod foo;");
        assert_eq!(unwrap_progress(p).extent, (0, 8))
    }

    #[test]
    fn item_trait() {
        let p = qp(item, "trait Foo {}");
        assert_eq!(unwrap_progress(p).extent(), (0, 12))
    }

    #[test]
    fn item_trait_public() {
        let p = qp(item, "pub trait Foo {}");
        assert_eq!(unwrap_progress(p).extent(), (0, 16))
    }

    #[test]
    fn item_trait_unsafe() {
        let p = qp(item, "unsafe trait Foo {}");
        assert_eq!(unwrap_progress(p).extent(), (0, 19))
    }

    #[test]
    fn item_trait_with_generics() {
        let p = qp(item, "trait Foo<T> {}");
        assert_eq!(unwrap_progress(p).extent(), (0, 15))
    }

    #[test]
    fn item_trait_with_members() {
        let p = qp(item, "trait Foo { fn bar(&self) -> u8; }");
        assert_eq!(unwrap_progress(p).extent(), (0, 34))
    }

    #[test]
    fn item_trait_with_members_with_patterns() {
        let p = qp(item, "trait Foo { fn bar(&self, &a: &u8) -> u8; }");
        assert_eq!(unwrap_progress(p).extent(), (0, 43))
    }

    #[test]
    fn item_trait_with_members_with_body() {
        let p = qp(item, "trait Foo { fn bar(&self) -> u8 { 42 } }");
        assert_eq!(unwrap_progress(p).extent(), (0, 40))
    }

    #[test]
    fn item_trait_with_unnamed_parameters() {
        let p = qp(item, "trait Foo { fn bar(&self, u8); }");
        assert_eq!(unwrap_progress(p).extent(), (0, 32))
    }

    #[test]
    fn item_trait_with_qualified_function() {
        let p = qp(item, r#"trait Foo { const unsafe extern "C" fn bar(); }"#);
        assert_eq!(unwrap_progress(p).extent(), (0, 47))
    }

    #[test]
    fn item_trait_with_associated_type() {
        let p = qp(item, "trait Foo { type Bar; }");
        assert_eq!(unwrap_progress(p).extent(), (0, 23))
    }

    #[test]
    fn item_trait_with_associated_type_with_bounds() {
        let p = qp(item, "trait Foo { type Bar: Baz; }");
        assert_eq!(unwrap_progress(p).extent(), (0, 28))
    }

    #[test]
    fn item_trait_with_supertraits() {
        let p = qp(item, "trait Foo: Bar + Baz {}");
        assert_eq!(unwrap_progress(p).extent(), (0, 23))
    }

    #[test]
    fn item_trait_with_where_clause() {
        let p = qp(item, "trait Foo where A: B {}");
        assert_eq!(unwrap_progress(p).extent(), (0, 23))
    }

    #[test]
    fn item_trait_all_space() {
        let p = qp(item, "trait Foo : Bar { type A : B ; fn a ( a : u8) -> u8 { a } }");
        assert_eq!(unwrap_progress(p).extent(), (0, 59))
    }

    #[test]
    fn item_type_alias() {
        let p = qp(item, "type Foo<T> = Bar<T, u8>;");
        assert_eq!(unwrap_progress(p).extent(), (0, 25))
    }

    #[test]
    fn item_type_alias_public() {
        let p = qp(item, "pub type Foo<T> = Bar<T, u8>;");
        assert_eq!(unwrap_progress(p).extent(), (0, 29))
    }

    #[test]
    fn item_const() {
        let p = qp(item, r#"const FOO: &'static str = "hi";"#);
        assert_eq!(unwrap_progress(p).extent(), (0, 31))
    }

    #[test]
    fn item_const_public() {
        let p = qp(item, "pub const FOO: u8 = 42;");
        assert_eq!(unwrap_progress(p).extent(), (0, 23))
    }

    #[test]
    fn item_static() {
        let p = qp(item, r#"static FOO: &'static str = "hi";"#);
        assert_eq!(unwrap_progress(p).extent(), (0, 32))
    }

    #[test]
    fn item_static_mut() {
        let p = qp(item, r#"static mut FOO: &'static str = "hi";"#);
        assert_eq!(unwrap_progress(p).extent(), (0, 36))
    }

    #[test]
    fn item_static_public() {
        let p = qp(item, "pub static FOO: u8 = 42;");
        assert_eq!(unwrap_progress(p).extent(), (0, 24))
    }

    #[test]
    fn item_extern_crate() {
        let p = qp(item, "extern crate foo;");
        assert_eq!(unwrap_progress(p).extent(), (0, 17))
    }

    #[test]
    fn item_extern_crate_public() {
        let p = qp(item, "pub extern crate foo;");
        assert_eq!(unwrap_progress(p).extent(), (0, 21))
    }

    #[test]
    fn item_extern_crate_rename() {
        let p = qp(item, "extern crate foo as bar;");
        assert_eq!(unwrap_progress(p).extent(), (0, 24))
    }

    #[test]
    fn item_extern_block() {
        let p = qp(item, r#"extern {}"#);
        assert_eq!(unwrap_progress(p).extent(), (0, 9))
    }

    #[test]
    fn item_extern_block_with_abi() {
        let p = qp(item, r#"extern "C" {}"#);
        assert_eq!(unwrap_progress(p).extent(), (0, 13))
    }

    #[test]
    fn item_extern_block_with_fn() {
        let p = qp(item, r#"extern { fn foo(bar: u8) -> bool; }"#);
        assert_eq!(unwrap_progress(p).extent(), (0, 35))
    }

    #[test]
    fn item_extern_block_with_variadic_fn() {
        let p = qp(item, r#"extern { fn foo(bar: u8, ...) -> bool; }"#);
        assert_eq!(unwrap_progress(p).extent(), (0, 40))
    }

    #[test]
    fn item_extern_block_with_attribute() {
        let p = qp(item, r#"extern { #[wow] }"#);
        assert_eq!(unwrap_progress(p).extent(), (0, 17))
    }

    #[test]
    fn impl_without_trait() {
        let p = qp(p_impl, "impl Bar {}");
        assert_eq!(unwrap_progress(p).extent, (0, 11))
    }

    #[test]
    fn impl_with_trait() {
        let p = qp(p_impl, "impl Foo for Bar {}");
        assert_eq!(unwrap_progress(p).extent, (0, 19))
    }

    #[test]
    fn impl_with_negative_trait() {
        let p = qp(p_impl, "impl !Foo for Bar {}");
        assert_eq!(unwrap_progress(p).extent, (0, 20))
    }

    #[test]
    fn impl_with_generics() {
        let p = qp(p_impl, "impl<'a, T> Foo<'a, T> for Bar<'a, T> {}");
        assert_eq!(unwrap_progress(p).extent, (0, 40))
    }

    #[test]
    fn impl_with_trait_bounds() {
        let p = qp(p_impl, "impl<T> Foo for Bar<T> where T: Quux {}");
        assert_eq!(unwrap_progress(p).extent, (0, 39))
    }

    #[test]
    fn impl_with_attribute() {
        let p = qp(p_impl, "impl Foo { #[attribute] fn bar() {} }");
        assert_eq!(unwrap_progress(p).extent, (0, 37))
    }

    #[test]
    fn impl_with_attributes() {
        let p = qp(p_impl, "impl Foo { #[a] #[b] fn bar() {} }");
        assert_eq!(unwrap_progress(p).extent, (0, 34))
    }

    #[test]
    fn impl_with_associated_type() {
        let p = qp(p_impl, "impl Foo { type A = B; }");
        assert_eq!(unwrap_progress(p).extent, (0, 24))
    }

    #[test]
    fn impl_with_unsafe() {
        let p = qp(p_impl, "unsafe impl Foo {}");
        assert_eq!(unwrap_progress(p).extent, (0, 18))
    }

    #[test]
    fn impl_with_macro_call() {
        let p = qp(p_impl, "impl Foo { bar!(); }");
        assert_eq!(unwrap_progress(p).extent, (0, 20))
    }

    #[test]
    fn enum_with_trailing_stuff() {
        let p = qp(p_enum, "enum A {} impl Foo for Bar {}");
        assert_eq!(unwrap_progress(p).extent, (0, 9))
    }

    #[test]
    fn enum_with_generic_types() {
        let p = qp(p_enum, "enum A { Foo(Vec<u8>) }");
        assert_eq!(unwrap_progress(p).extent, (0, 23))
    }

    #[test]
    fn enum_with_generic_declarations() {
        let p = qp(p_enum, "enum A<T> { Foo(Vec<T>) }");
        assert_eq!(unwrap_progress(p).extent, (0, 25))
    }

    #[test]
    fn enum_with_struct_variant() {
        let p = qp(p_enum, "enum A { Foo { a: u8 } }");
        assert_eq!(unwrap_progress(p).extent, (0, 24))
    }

    #[test]
    fn enum_with_attribute() {
        let p = qp(p_enum, "enum Foo { #[attr] A(u8)}");
        assert_eq!(unwrap_progress(p).extent, (0, 25))
    }

    #[test]
    fn enum_with_attribute_on_value() {
        let p = qp(p_enum, "enum Foo { A(#[attr] u8) }");
        assert_eq!(unwrap_progress(p).extent, (0, 26))
    }

    #[test]
    fn enum_with_discriminant() {
        let p = qp(p_enum, "enum Foo { A = 1, B = 2 }");
        assert_eq!(unwrap_progress(p).extent, (0, 25))
    }

    #[test]
    fn enum_with_where_clause() {
        let p = qp(p_enum, "enum Foo<A> where A: Bar { Z }");
        assert_eq!(unwrap_progress(p).extent, (0, 30))
    }

    #[test]
    fn enum_public() {
        let p = qp(p_enum, "pub enum A {}");
        assert_eq!(unwrap_progress(p).extent, (0, 13))
    }

    #[test]
    fn fn_with_public_modifier() {
        let p = qp(function_header, "pub fn foo()");
        assert_eq!(unwrap_progress(p).extent, (0, 12))
    }

    #[test]
    fn fn_with_const_modifier() {
        let p = qp(function_header, "const fn foo()");
        assert_eq!(unwrap_progress(p).extent, (0, 14))
    }

    #[test]
    fn fn_with_extern_modifier() {
        let p = qp(function_header, "extern fn foo()");
        assert_eq!(unwrap_progress(p).extent, (0, 15))
    }

    #[test]
    fn fn_with_extern_modifier_and_abi() {
        let p = qp(function_header, r#"extern "C" fn foo()"#);
        assert_eq!(unwrap_progress(p).extent, (0, 19))
    }

    #[test]
    fn fn_with_self_type_reference() {
        let p = qp(function_header, "fn foo(&self)");
        assert_eq!(unwrap_progress(p).extent, (0, 13))
    }

    #[test]
    fn fn_with_self_type_value() {
        let p = qp(function_header, "fn foo(self)");
        assert_eq!(unwrap_progress(p).extent, (0, 12))
    }

    #[test]
    fn fn_with_self_type_value_mut() {
        let p = qp(function_header, "fn foo(mut self)");
        assert_eq!(unwrap_progress(p).extent, (0, 16))
    }

    #[test]
    fn fn_with_self_type_reference_mut() {
        let p = qp(function_header, "fn foo(&mut self)");
        assert_eq!(unwrap_progress(p).extent, (0, 17))
    }

    #[test]
    fn fn_with_self_type_with_lifetime() {
        let p = qp(function_header, "fn foo<'a>(&'a self)");
        assert_eq!(unwrap_progress(p).extent, (0, 20))
    }

    #[test]
    fn fn_with_self_type_and_regular() {
        let p = qp(function_header, "fn foo(&self, a: u8)");
        assert_eq!(unwrap_progress(p).extent, (0, 20))
    }

    #[test]
    fn fn_with_self_type_explicit_type() {
        let p = qp(function_header, "fn foo(self: &mut Self)");
        assert_eq!(unwrap_progress(p).extent, (0, 23))
    }

    #[test]
    fn fn_with_argument() {
        let p = qp(function_header, "fn foo(a: u8)");
        assert_eq!(unwrap_progress(p).extent, (0, 13))
    }

    #[test]
    fn fn_with_arguments_all_space() {
        let p = qp(function_header, "fn foo ( a : u8 )");
        assert_eq!(unwrap_progress(p).extent, (0, 17))
    }

    #[test]
    fn fn_with_argument_with_generic() {
        let p = qp(function_header, "fn foo(a: Vec<u8>)");
        assert_eq!(unwrap_progress(p).extent, (0, 18))
    }

    #[test]
    fn fn_with_arguments() {
        let p = qp(function_header, "fn foo(a: u8, b: u8)");
        assert_eq!(unwrap_progress(p).extent, (0, 20))
    }

    #[test]
    fn fn_with_arguments_with_patterns() {
        let p = qp(function_header, "fn foo(&a: &u8)");
        assert_eq!(unwrap_progress(p).extent, (0, 15))
    }

    #[test]
    fn fn_with_return_type() {
        let p = qp(function_header, "fn foo() -> bool");
        assert_eq!(unwrap_progress(p).extent, (0, 16))
    }

    #[test]
    fn fn_with_generics() {
        let p = qp(function_header, "fn foo<A, B>()");
        assert_eq!(unwrap_progress(p).extent, (0, 14))
    }

    #[test]
    fn fn_with_lifetimes() {
        let p = qp(function_header, "fn foo<'a, 'b>()");
        assert_eq!(unwrap_progress(p).extent, (0, 16))
    }

    #[test]
    fn fn_with_lifetimes_and_generics() {
        let p = qp(function_header, "fn foo<'a, T>()");
        assert_eq!(unwrap_progress(p).extent, (0, 15))
    }

    #[test]
    fn fn_with_whitespace_before_arguments() {
        let p = qp(function_header, "fn foo () -> ()");
        assert_eq!(unwrap_progress(p).extent, (0, 15))
    }

    #[test]
    fn fn_with_whitespace_before_generics() {
        let p = qp(function_header, "fn foo <'a, T>() -> ()");
        assert_eq!(unwrap_progress(p).extent, (0, 22))
    }

    #[test]
    fn fn_with_unsafe_qualifier() {
        let p = qp(function_header, "unsafe fn foo()");
        assert_eq!(unwrap_progress(p).extent, (0, 15))
    }

    #[test]
    fn block_with_multiple_implicit_statement_macro_calls() {
        let p = qp(block, "{ a! {} b! {} }");
        assert_eq!(unwrap_progress(p).extent, (0, 15));
    }

    #[test]
    fn block_promotes_implicit_statement_to_expression() {
        let p = qp(block, "{ if a {} }");
        let p = unwrap_progress(p);
        assert!(p.statements.is_empty());
        assert_eq!(p.expression.unwrap().extent(), (2, 9));
    }

    #[test]
    fn statement_match_no_semicolon() {
        let p = qp(statement, "match a { _ => () }");
        assert_eq!(unwrap_progress(p).into_expression().unwrap().extent(), (0, 19))
    }

    #[test]
    fn statement_use() {
        let p = qp(statement, "use foo::Bar;");
        assert_eq!(unwrap_progress(p).extent(), (0, 13))
    }

    #[test]
    fn statement_any_item() {
        let p = qp(statement, "struct Foo {}");
        assert_eq!(unwrap_progress(p).extent(), (0, 13))
    }

    #[test]
    fn statement_braced_expression_followed_by_method() {
        let p = qp(statement, "match 1 { _ => 1u8 }.count_ones()");
        assert_eq!(unwrap_progress(p).extent(), (0, 33))
    }

    #[test]
    fn expr_true() {
        let p = qp(expression, "true");
        assert_eq!(unwrap_progress(p).extent(), (0, 4))
    }

    #[test]
    fn expr_number_binary() {
        let p = qp(expression, "0x0101");
        assert_eq!(unwrap_progress(p).extent(), (0, 6))
    }

    #[test]
    fn expr_number_decimal() {
        let p = qp(expression, "123");
        assert_eq!(unwrap_progress(p).extent(), (0, 3))
    }

    #[test]
    fn expr_number_hexadecimal() {
        let p = qp(expression, "0xDEADBEEF");
        assert_eq!(unwrap_progress(p).extent(), (0, 10))
    }

    #[test]
    fn expr_number_octal() {
        let p = qp(expression, "0o777");
        assert_eq!(unwrap_progress(p).extent(), (0, 5))
    }

    #[test]
    fn expr_number_with_spacers() {
        let p = qp(expression, "1_000_000");
        assert_eq!(unwrap_progress(p).extent(), (0, 9))
    }

    #[test]
    fn expr_let_explicit_type() {
        let p = qp(expression, "let foo: bool");
        assert_eq!(unwrap_progress(p).extent(), (0, 13))
    }

    #[test]
    fn expr_let_mut() {
        let p = qp(expression, "let mut pm = Master::new()");
        assert_eq!(unwrap_progress(p).extent(), (0, 26))
    }

    #[test]
    fn expr_let_no_value() {
        let p = qp(expression, "let pm");
        assert_eq!(unwrap_progress(p).extent(), (0, 6))
    }

    #[test]
    fn expr_assign() {
        let p = qp(expression, "a = b");
        assert_eq!(unwrap_progress(p).extent(), (0, 5))
    }

    #[test]
    fn expr_assign_to_field() {
        let p = qp(expression, "a.b = c");
        assert_eq!(unwrap_progress(p).extent(), (0, 7))
    }

    #[test]
    fn expr_value_with_path() {
        let p = qp(expression, "Master::new()");
        assert_eq!(unwrap_progress(p).extent(), (0, 13))
    }

    #[test]
    fn expr_field_access_name() {
        let p = qp(expression, "foo.bar");
        assert_eq!(unwrap_progress(p).extent(), (0, 7))
    }

    #[test]
    fn expr_field_access_number() {
        let p = qp(expression, "foo.0");
        assert_eq!(unwrap_progress(p).extent(), (0, 5))
    }

    #[test]
    fn expr_field_access_multiple() {
        let p = qp(expression, "foo.bar.baz");
        assert_eq!(unwrap_progress(p).extent(), (0, 11))
    }

    #[test]
    fn expr_call_function() {
        let p = unwrap_progress(qp(expression, "foo()"));
        assert!(p.is_call());
        assert_eq!(p.extent(), (0, 5))
    }

    #[test]
    fn expr_call_method() {
        let p = unwrap_progress(qp(expression, "foo.bar()"));
        assert!(p.is_call());
        assert_eq!(p.extent(), (0, 9))
    }

    #[test]
    fn expr_call_method_multiple() {
        let p = unwrap_progress(qp(expression, "foo.bar().baz()"));
        assert!(p.is_call());
        assert_eq!(p.extent(), (0, 15))
    }

    #[test]
    fn expr_call_method_multiple_spaced() {
        let p = unwrap_progress(qp(expression, "foo.bar()\n    .baz()"));
        assert!(p.is_call());
        assert_eq!(p.extent(), (0, 20))
    }

    #[test]
    fn expr_call_method_with_turbofish() {
        let p = unwrap_progress(qp(expression, "foo.bar::<u8>()"));
        assert!(p.is_call());
        assert_eq!(p.extent(), (0, 15))
    }

    #[test]
    fn expr_call_method_with_turbofish_nested() {
        let p = unwrap_progress(qp(expression, "e.into_iter().collect::<BTreeSet<_>>()"));
        assert!(p.is_call());
        assert_eq!(p.extent(), (0, 38))
    }

    #[test]
    fn expr_call_method_with_turbofish_on_type_and_method() {
        let p = unwrap_progress(qp(expression, "Foo::<u8>::bar()"));
        assert!(p.is_call());
        assert_eq!(p.extent(), (0, 16))
    }

    #[test]
    fn expr_call_of_expr() {
        let p = unwrap_progress(qp(expression, "{foo}()"));
        assert!(p.is_call());
        assert_eq!(p.extent(), (0, 7))
    }

    #[test]
    fn expr_call_all_space() {
        let p = unwrap_progress(qp(expression, "foo ( true , false )"));
        assert!(p.is_call());
        assert_eq!(p.extent(), (0, 20))
    }

    #[test]
    fn expr_for_loop() {
        let p = qp(expression, "for (a, b) in c {}");
        assert_eq!(unwrap_progress(p).extent(), (0, 18))
    }

    #[test]
    fn expr_for_loop_with_label() {
        let p = qp(expression, "'a: for (a, b) in c {}");
        assert_eq!(unwrap_progress(p).extent(), (0, 22))
    }

    #[test]
    fn expr_loop() {
        let p = qp(expression, "loop {}");
        assert_eq!(unwrap_progress(p).extent(), (0, 7))
    }

    #[test]
    fn expr_loop_with_label() {
        let p = qp(expression, "'a: loop {}");
        assert_eq!(unwrap_progress(p).extent(), (0, 11))
    }

    #[test]
    fn expr_match() {
        let p = qp(expression, "match foo { _ => () }");
        assert_eq!(unwrap_progress(p).extent(), (0, 21))
    }

    #[test]
    fn expr_match_brace_with_no_comma_followed_by_tuple_isnt_a_function_call() {
        // `_ => {} (_,)` is unambigous from a function call
        // `{foo}(arg)`. We must check blocks specifically.
        let p = qp(expression, "match (1,) { (1,) => {} (_,) => {} }");
        assert_eq!(unwrap_progress(p).extent(), (0, 36))
    }

    #[test]
    fn expr_match_expr_trailing_comma_and_whitespace() {
        let p = qp(expression, "match 1 { 1 => 2, _ => 3, }");
        assert_eq!(unwrap_progress(p).extent(), (0, 27))
    }

    #[test]
    fn expr_match_head_followed_by_block() {
        let p = qp(expression, "match foo {}");
        assert_eq!(unwrap_progress(p).extent(), (0, 12))
    }

    #[test]
    fn expr_tuple() {
        let p = qp(expression, "(1, 2)");
        let t = unwrap_progress(p);
        assert_eq!(t.extent(), (0, 6));
        assert!(t.is_tuple())
    }

    #[test]
    fn expr_tuple_of_none() {
        let p = qp(expression, "()");
        let t = unwrap_progress(p);
        assert_eq!(t.extent(), (0, 2));
        assert!(t.is_tuple())
    }

    #[test]
    fn expr_tuple_of_one() {
        let p = qp(expression, "(1,)");
        let t = unwrap_progress(p);
        assert_eq!(t.extent(), (0, 4));
        assert!(t.is_tuple())
    }

    #[test]
    fn expr_tuple_all_space() {
        let p = qp(expression, "( 1 , 2 )");
        let t = unwrap_progress(p);
        assert_eq!(t.extent(), (0, 9));
        assert!(t.is_tuple())
    }

    #[test]
    fn expr_parens() {
        let p = qp(expression, "(a && b)");
        let t = unwrap_progress(p);
        assert_eq!(t.extent(), (0, 8));
        assert!(t.is_parenthetical())
    }

    #[test]
    fn expr_parens_with_one_value_is_not_tuple() {
        let p = qp(expression, "(1)");
        let t = unwrap_progress(p);
        assert_eq!(t.extent(), (0, 3));
        assert!(t.is_parenthetical())
    }

    #[test]
    fn expr_parens_all_space() {
        let p = qp(expression, "( 1 )");
        let t = unwrap_progress(p);
        assert_eq!(t.extent(), (0, 5));
        assert!(t.is_parenthetical())
    }

    #[test]
    fn expr_block() {
        let p = qp(expression, "{}");
        assert_eq!(unwrap_progress(p).extent(), (0, 2))
    }

    #[test]
    fn expr_unsafe_block() {
        let p = qp(expression, "unsafe {}");
        assert_eq!(unwrap_progress(p).extent(), (0, 9))
    }

    #[test]
    fn expr_if_() {
        let p = qp(expression, "if a {}");
        assert_eq!(unwrap_progress(p).extent(), (0, 7))
    }

    #[test]
    fn expr_if_else() {
        let p = qp(expression, "if a {} else {}");
        assert_eq!(unwrap_progress(p).extent(), (0, 15))
    }

    #[test]
    fn expr_if_else_if() {
        let p = qp(expression, "if a {} else if b {}");
        assert_eq!(unwrap_progress(p).extent(), (0, 20))
    }

    #[test]
    fn expr_if_let() {
        let p = qp(expression, "if let Some(a) = None {}");
        assert_eq!(unwrap_progress(p).extent(), (0, 24))
    }

    #[test]
    fn expr_while() {
        let p = qp(expression, "while is_awesome() {}");
        assert_eq!(unwrap_progress(p).extent(), (0, 21))
    }

    #[test]
    fn expr_while_with_label() {
        let p = qp(expression, "'a: while is_awesome() {}");
        assert_eq!(unwrap_progress(p).extent(), (0, 25))
    }

    #[test]
    fn expr_while_let() {
        let p = qp(expression, "while let Some(a) = None {}");
        assert_eq!(unwrap_progress(p).extent(), (0, 27))
    }

    #[test]
    fn expr_while_let_with_label() {
        let p = qp(expression, "'a: while let Some(a) = None {}");
        assert_eq!(unwrap_progress(p).extent(), (0, 31))
    }

    #[test]
    fn expr_binary_op() {
        let p = qp(expression, "a < b");
        assert_eq!(unwrap_progress(p).extent(), (0, 5))
    }

    #[test]
    fn expr_binary_multiple() {
        let p = qp(expression, "1 + 2 + 3");
        assert_eq!(unwrap_progress(p).extent(), (0, 9))
    }

    #[test]
    fn expr_binary_op_two_char() {
        let p = qp(expression, "a >= b");
        assert_eq!(unwrap_progress(p).extent(), (0, 6))
    }

    #[test]
    fn expr_binary_op_equality() {
        let p = qp(expression, "a == b != c");
        assert_eq!(unwrap_progress(p).extent(), (0, 11))
    }

    #[test]
    fn expr_binary_op_math() {
        let p = qp(expression, "a + b - c / d % e");
        assert_eq!(unwrap_progress(p).extent(), (0, 17))
    }

    #[test]
    fn expr_binary_op_boolean_logic() {
        let p = qp(expression, "a && b || c");
        assert_eq!(unwrap_progress(p).extent(), (0, 11))
    }

    #[test]
    fn expr_binary_op_shifting() {
        let p = qp(expression, "a >> b << c");
        assert_eq!(unwrap_progress(p).extent(), (0, 11))
    }

    #[test]
    fn expr_binary_op_shift_assign() {
        let p = qp(expression, "a >>= b <<= c");
        assert_eq!(unwrap_progress(p).extent(), (0, 13))
    }

    #[test]
    fn expr_binary_op_bitwise() {
        let p = qp(expression, "a & b | c ^ d");
        assert_eq!(unwrap_progress(p).extent(), (0, 13))
    }

    #[test]
    fn expr_binary_op_bitwise_assign() {
        let p = qp(expression, "a &= b |= c ^= d");
        assert_eq!(unwrap_progress(p).extent(), (0, 16))
    }

    #[test]
    fn expr_braced_true() {
        let p = qp(expression, "{ true }");
        assert_eq!(unwrap_progress(p).extent(), (0, 8))
    }

    #[test]
    fn expr_macro_call_with_nested_parens() {
        let p = qp(expression, "foo!(())");
        assert_eq!(unwrap_progress(p).extent(), (0, 8))
    }

    #[test]
    fn expr_macro_call_with_quoted_parens() {
        let p = qp(expression, r#"foo!("(")"#);
        assert_eq!(unwrap_progress(p).extent(), (0, 9))
    }

    #[test]
    fn expr_macro_call_with_square_brackets() {
        let p = qp(expression, "vec![]");
        assert_eq!(unwrap_progress(p).extent(), (0, 6))
    }

    #[test]
    fn expr_macro_call_with_curly_brackets() {
        let p = qp(expression, "foo! { }");
        assert_eq!(unwrap_progress(p).extent(), (0, 8))
    }

    #[test]
    fn expr_macro_call_with_ident() {
        let p = qp(expression, "macro_rules! foo { }");
        assert_eq!(unwrap_progress(p).extent(), (0, 20))
    }

    #[test]
    fn expr_range_both() {
        let p = qp(expression, "1..2");
        assert_eq!(unwrap_progress(p).extent(), (0, 4))
    }

    #[test]
    fn expr_range_left() {
        let p = qp(expression, "3..");
        assert_eq!(unwrap_progress(p).extent(), (0, 3))
    }

    #[test]
    fn expr_range_right() {
        let p = qp(expression, "..4");
        assert_eq!(unwrap_progress(p).extent(), (0, 3))
    }

    #[test]
    fn expr_range_none() {
        let p = qp(expression, "..");
        assert_eq!(unwrap_progress(p).extent(), (0, 2))
    }

    #[test]
    fn expr_range_all_space() {
        let p = qp(expression, ".. 2");
        assert_eq!(unwrap_progress(p).extent(), (0, 4))
    }

    #[test]
    fn expr_range_tail_all_space() {
        let p = qp(expression, "1 .. 2");
        assert_eq!(unwrap_progress(p).extent(), (0, 6))
    }

    #[test]
    fn expr_value_struct_literal() {
        let p = qp(expression, "Point { a: 1 }");
        assert_eq!(unwrap_progress(p).extent(), (0, 14))
    }

    #[test]
    fn expr_value_struct_literal_shorthand() {
        let p = qp(expression, "Point { a }");
        assert_eq!(unwrap_progress(p).extent(), (0, 11))
    }

    #[test]
    fn expr_value_struct_literal_with_splat() {
        let p = qp(expression, "Point { x: 1, ..point }");
        assert_eq!(unwrap_progress(p).extent(), (0, 23))
    }

    #[test]
    fn expr_value_starts_with_keyword() {
        let p = qp(expression, "continuez");
        assert_eq!(unwrap_progress(p).extent(), (0, 9));
    }

    #[test]
    fn expr_closure() {
        let p = qp(expression, "|a| a");
        assert_eq!(unwrap_progress(p).extent(), (0, 5))
    }

    #[test]
    fn expr_closure_multiple() {
        let p = qp(expression, "|a, b| a + b");
        assert_eq!(unwrap_progress(p).extent(), (0, 12))
    }

    #[test]
    fn expr_closure_explicit_type() {
        let p = qp(expression, "|a: u8| a");
        assert_eq!(unwrap_progress(p).extent(), (0, 9))
    }

    #[test]
    fn expr_closure_return_type() {
        let p = qp(expression, "|a| -> u8 { a }");
        assert_eq!(unwrap_progress(p).extent(), (0, 15))
    }

    #[test]
    fn expr_closure_pattern() {
        let p = qp(expression, "|&a| a");
        assert_eq!(unwrap_progress(p).extent(), (0, 6))
    }

    #[test]
    fn expr_closure_move() {
        let p = qp(expression, "move || 42");
        assert_eq!(unwrap_progress(p).extent(), (0, 10))
    }

    #[test]
    fn expr_closure_all_space() {
        let p = qp(expression, "move | a : u8 | -> u8 { a }");
        assert_eq!(unwrap_progress(p).extent(), (0, 27))
    }

    #[test]
    fn expr_return() {
        let p = qp(expression, "return 1");
        assert_eq!(unwrap_progress(p).extent(), (0, 8))
    }

    #[test]
    fn expr_return_no_value() {
        let p = qp(expression, "return");
        assert_eq!(unwrap_progress(p).extent(), (0, 6))
    }

    #[test]
    fn expr_continue() {
        let p = unwrap_progress(qp(expression, "continue"));
        assert!(p.is_continue());
        assert_eq!(p.extent(), (0, 8))
    }

    #[test]
    fn expr_continue_with_label() {
        let p = unwrap_progress(qp(expression, "continue 'outer"));
        assert!(p.is_continue());
        assert_eq!(p.extent(), (0, 15))
    }

    #[test]
    fn expr_break() {
        let p = unwrap_progress(qp(expression, "break"));
        assert!(p.is_break());
        assert_eq!(p.extent(), (0, 5))
    }

    #[test]
    fn expr_break_with_label() {
        let p = unwrap_progress(qp(expression, "break 'outer"));
        assert!(p.is_break());
        assert_eq!(p.extent(), (0, 12))
    }

    #[test]
    fn expr_array_explicit() {
        let p = qp(expression, "[1, 1]");
        assert_eq!(unwrap_progress(p).extent(), (0, 6))
    }

    #[test]
    fn expr_array_repeated() {
        let p = qp(expression, "[1; 2*3]");
        assert_eq!(unwrap_progress(p).extent(), (0, 8))
    }

    #[test]
    fn expr_array_explicit_all_space() {
        let p = qp(expression, "[ 1 , 1 ]");
        assert_eq!(unwrap_progress(p).extent(), (0, 9))
    }

    #[test]
    fn expr_array_repeated_all_space() {
        let p = qp(expression, "[ 1 ; 2 * 3 ]");
        assert_eq!(unwrap_progress(p).extent(), (0, 13))
    }

    #[test]
    fn expr_char_literal() {
        let p = qp(expression, "'a'");
        assert_eq!(unwrap_progress(p).extent(), (0, 3))
    }

    #[test]
    fn expr_char_literal_escape() {
        let p = qp(expression, r"'\''");
        assert_eq!(unwrap_progress(p).extent(), (0, 4))
    }

    #[test]
    fn expr_string_literal() {
        let p = qp(expression, r#""a""#);
        assert_eq!(unwrap_progress(p).extent(), (0, 3))
    }

    #[test]
    fn expr_string_literal_escape() {
        let p = qp(expression, r#""\"""#);
        assert_eq!(unwrap_progress(p).extent(), (0, 4))
    }

    #[test]
    fn expr_string_literal_raw() {
        let p = qp(expression, r###"r#"foo"#"###);
        assert_eq!(unwrap_progress(p).extent(), (0, 8))
    }

    #[test]
    fn expr_slice_index() {
        let p = qp(expression, "a[..2]");
        assert_eq!(unwrap_progress(p).extent(), (0, 6))
    }

    #[test]
    fn expr_reference() {
        let p = qp(expression, "&foo");
        assert_eq!(unwrap_progress(p).extent(), (0, 4))
    }

    #[test]
    fn expr_reference_mut() {
        let p = qp(expression, "&mut foo");
        assert_eq!(unwrap_progress(p).extent(), (0, 8))
    }

    #[test]
    fn expr_dereference() {
        let p = qp(expression, "*foo");
        assert_eq!(unwrap_progress(p).extent(), (0, 4))
    }

    #[test]
    fn expr_unary_not() {
        let p = qp(expression, "!foo");
        assert_eq!(unwrap_progress(p).extent(), (0, 4))
    }

    #[test]
    fn expr_unary_negate() {
        let p = qp(expression, "-foo");
        assert_eq!(unwrap_progress(p).extent(), (0, 4))
    }

    #[test]
    fn expr_as_type() {
        let p = qp(expression, "42 as u8");
        assert_eq!(unwrap_progress(p).extent(), (0, 8))
    }

    #[test]
    fn expr_as_type_followed_by_addition() {
        let p = unwrap_progress(qp(expression, "42 as u8 + 1"));
        let p = p.into_binary().unwrap();
        assert!(p.lhs.is_as_type());
        assert_eq!(p.extent, (0, 12));
    }

    #[test]
    fn expr_as_type_of_value() {
        let p = qp(expression, "bits as u64");
        assert_eq!(unwrap_progress(p).extent(), (0, 11))
    }

    #[test]
    fn expr_try_operator() {
        let p = qp(expression, "foo?");
        assert_eq!(unwrap_progress(p).extent(), (0, 4))
    }

    #[test]
    fn expr_try_operator_all_space() {
        let p = qp(expression, "foo ?");
        assert_eq!(unwrap_progress(p).extent(), (0, 5))
    }

    #[test]
    fn expr_box() {
        let p = qp(expression, "box foo");
        assert_eq!(unwrap_progress(p).extent(), (0, 7))
    }

    #[test]
    fn expr_byte_string() {
        let p = qp(expression, r#"b"hello""#);
        assert_eq!(unwrap_progress(p).extent(), (0, 8))
    }

    #[test]
    fn expr_byte_string_escape() {
        let p = qp(expression, r#"b"he\"llo""#);
        assert_eq!(unwrap_progress(p).extent(), (0, 10))
    }

    #[test]
    fn expr_byte_string_raw() {
        let p = qp(expression, r###"br#"hello"#"###);
        assert_eq!(unwrap_progress(p).extent(), (0, 11))
    }

    #[test]
    fn expr_byte() {
        let p = qp(expression, r#"b'a'"#);
        assert_eq!(unwrap_progress(p).extent(), (0, 4))
    }

    #[test]
    fn expr_byte_escape() {
        let p = qp(expression, r#"b'\''"#);
        assert_eq!(unwrap_progress(p).extent(), (0, 5))
    }

    #[test]
    fn expr_disambiguation() {
        let p = qp(expression, "<Foo as Bar>::quux");
        assert_eq!(unwrap_progress(p).extent(), (0, 18))
    }

    #[test]
    fn expr_disambiguation_without_disambiguation() {
        let p = qp(expression, "<Foo>::quux");
        assert_eq!(unwrap_progress(p).extent(), (0, 11))
    }

    #[test]
    fn expr_followed_by_block_disallows_struct_literal() {
        let p = qp(expr_followed_by_block, "a {}");
        let (e, b) = unwrap_progress(p);
        assert_eq!(e.extent(), (0, 1));
        assert_eq!(b.extent, (2, 4));
    }

    #[test]
    fn expr_followed_by_block_with_compound_condition() {
        let p = qp(expr_followed_by_block, "a && b {}");
        let (e, b) = unwrap_progress(p);
        assert_eq!(e.extent(), (0, 6));
        assert_eq!(b.extent, (7, 9));
    }

    #[test]
    fn expr_followed_by_block_with_parenthesized_struct_literal() {
        let p = qp(expr_followed_by_block, "(a {}) {}");
        let (e, b) = unwrap_progress(p);
        assert_eq!(e.extent(), (0, 6));
        let p = e.into_parenthetical().unwrap();
        assert!(p.expression.is_value());
        assert_eq!(b.extent, (7, 9));
    }

    #[test]
    fn pathed_ident_with_leading_separator() {
        let p = qp(pathed_ident, "::foo");
        assert_eq!(unwrap_progress(p).extent, (0, 5))
    }

    #[test]
    fn pathed_ident_with_turbofish() {
        let p = qp(pathed_ident, "foo::<Vec<u8>>");
        assert_eq!(unwrap_progress(p).extent, (0, 14))
    }

    #[test]
    fn pathed_ident_all_space() {
        let p = qp(pathed_ident, "foo :: < Vec < u8 > , Option < bool > >");
        assert_eq!(unwrap_progress(p).extent, (0, 39))
    }

    #[test]
    fn number_decimal_cannot_start_with_underscore() {
        let p = qp(number_literal, "_123");
        let (err_loc, errs) = unwrap_progress_err(p);
        assert_eq!(err_loc, 0);
        assert!(errs.contains(&Error::ExpectedNumber));
    }

    #[test]
    fn number_with_exponent() {
        let p = qp(number_literal, "1e2");
        assert_eq!(unwrap_progress(p).extent(), (0, 3))
    }

    #[test]
    fn number_with_prefix_and_exponent() {
        let p = qp(number_literal, "0x1e2");
        assert_eq!(unwrap_progress(p).extent(), (0, 5))
    }

    #[test]
    fn number_with_fractional() {
        let p = qp(number_literal, "1.2");
        assert_eq!(unwrap_progress(p).extent(), (0, 3))
    }

    #[test]
    fn number_with_fractional_with_suffix() {
        let p = qp(number_literal, "1.2f32");
        assert_eq!(unwrap_progress(p).extent(), (0, 6))
    }

    #[test]
    fn number_with_prefix_and_fractional() {
        let p = qp(number_literal, "0x1.2");
        assert_eq!(unwrap_progress(p).extent(), (0, 5))
    }

    #[test]
    fn number_with_prefix_exponent_and_fractional() {
        let p = qp(number_literal, "0o7.3e9");
        assert_eq!(unwrap_progress(p).extent(), (0, 7))
    }

    #[test]
    fn number_with_prefix_can_have_underscore_after_prefix() {
        let p = qp(number_literal, "0x_123");
        assert_eq!(unwrap_progress(p).extent(), (0, 6))
    }

    #[test]
    fn number_binary_can_have_suffix() {
        let p = qp(number_literal, "0b111u8");
        assert_eq!(unwrap_progress(p).extent(), (0, 7))
    }

    #[test]
    fn number_decimal_can_have_suffix() {
        let p = qp(number_literal, "123i16");
        assert_eq!(unwrap_progress(p).extent(), (0, 6))
    }

    #[test]
    fn number_hexadecimal_can_have_suffix() {
        let p = qp(number_literal, "0xBEEF__u32");
        assert_eq!(unwrap_progress(p).extent(), (0, 11))
    }

    #[test]
    fn number_octal_can_have_suffix() {
        let p = qp(number_literal, "0o777_isize");
        assert_eq!(unwrap_progress(p).extent(), (0, 11))
    }

    #[test]
    fn pattern_with_path() {
        let p = qp(pattern, "foo::Bar::Baz");
        assert_eq!(unwrap_progress(p).extent(), (0, 13))
    }

    #[test]
    fn pattern_with_ref() {
        let p = qp(pattern, "ref a");
        assert_eq!(unwrap_progress(p).extent(), (0, 5))
    }

    #[test]
    fn pattern_with_tuple() {
        let p = qp(pattern, "(a, b)");
        assert_eq!(unwrap_progress(p).extent(), (0, 6))
    }

    #[test]
    fn pattern_with_enum_tuple() {
        let p = qp(pattern, "Baz(a)");
        assert_eq!(unwrap_progress(p).extent(), (0, 6))
    }

    #[test]
    fn pattern_with_tuple_wildcard() {
        let p = qp(pattern, "(..)");
        assert_eq!(unwrap_progress(p).extent(), (0, 4))
    }

    #[test]
    fn pattern_with_tuple_wildcard_anywhere() {
        let p = qp(pattern, "(a, .., b)");
        assert_eq!(unwrap_progress(p).extent(), (0, 10))
    }

    #[test]
    fn pattern_with_tuple_all_space() {
        let p = qp(pattern, "( a , .. , b )");
        assert_eq!(unwrap_progress(p).extent(), (0, 14))
    }

    #[test]
    fn pattern_with_enum_struct() {
        let p = qp(pattern, "Baz { a: a }");
        assert_eq!(unwrap_progress(p).extent(), (0, 12))
    }

    #[test]
    fn pattern_with_enum_struct_shorthand() {
        let p = qp(pattern, "Baz { a }");
        assert_eq!(unwrap_progress(p).extent(), (0, 9))
    }

    #[test]
    fn pattern_with_enum_struct_shorthand_with_ref() {
        let p = qp(pattern, "Baz { ref a }");
        assert_eq!(unwrap_progress(p).extent(), (0, 13))
    }

    #[test]
    fn pattern_with_enum_struct_wildcard() {
        let p = qp(pattern, "Baz { .. }");
        assert_eq!(unwrap_progress(p).extent(), (0, 10))
    }

    #[test]
    fn pattern_with_byte_literal() {
        let p = qp(pattern, "b'a'");
        assert_eq!(unwrap_progress(p).extent(), (0, 4))
    }

    #[test]
    fn pattern_with_char_literal() {
        let p = qp(pattern, "'a'");
        assert_eq!(unwrap_progress(p).extent(), (0, 3))
    }

    #[test]
    fn pattern_with_byte_string_literal() {
        let p = qp(pattern, r#"b"hello""#);
        assert_eq!(unwrap_progress(p).extent(), (0, 8))
    }

    #[test]
    fn pattern_with_string_literal() {
        let p = qp(pattern, r#""hello""#);
        assert_eq!(unwrap_progress(p).extent(), (0, 7))
    }

    #[test]
    fn pattern_with_numeric_literal() {
        let p = qp(pattern, "42");
        assert_eq!(unwrap_progress(p).extent(), (0, 2))
    }

    #[test]
    fn pattern_with_reference() {
        let p = qp(pattern, "&a");
        assert_eq!(unwrap_progress(p).extent(), (0, 2))
    }

    #[test]
    fn pattern_with_reference_mutable() {
        let p = unwrap_progress(qp(pattern, "&mut ()"));
        assert!(p.kind.is_reference());
        assert_eq!(p.extent(), (0, 7));
    }

    #[test]
    fn pattern_with_named_subpattern() {
        let p = unwrap_progress(qp(pattern, "a @ 1"));
        assert_eq!(p.extent(), (0, 5));
    }

    #[test]
    fn pattern_with_named_subpattern_qualifiers() {
        let p = unwrap_progress(qp(pattern, "ref mut a @ 1"));
        assert_eq!(p.extent(), (0, 13));
    }

    #[test]
    fn pattern_with_numeric_range() {
        let p = qp(pattern, "1 ... 10");
        assert_eq!(unwrap_progress(p).extent(), (0, 8))
    }

    #[test]
    fn pattern_with_character_range() {
        let p = qp(pattern, "'a'...'z'");
        assert_eq!(unwrap_progress(p).extent(), (0, 9))
    }

    #[test]
    fn pattern_with_byte_range() {
        let p = qp(pattern, "b'a'...b'z'");
        assert_eq!(unwrap_progress(p).extent(), (0, 11))
    }

    #[test]
    fn match_arm_with_alternate() {
        let p = qp(match_arm, "a | b => 1");
        assert_eq!(unwrap_progress(p).extent, (0, 10))
    }

    #[test]
    fn match_arm_with_guard() {
        let p = qp(match_arm, "a if a > 2 => 1");
        assert_eq!(unwrap_progress(p).extent, (0, 15))
    }

    #[test]
    fn match_arm_with_attribute() {
        let p = qp(match_arm, "#[cfg(cool)] _ => 1");
        assert_eq!(unwrap_progress(p).extent, (0, 19))
    }

    #[test]
    fn type_tuple() {
        let p = qp(typ, "(u8, u8)");
        assert_eq!(unwrap_progress(p).extent(), (0, 8))
    }

    #[test]
    fn type_tuple_all_space() {
        let p = qp(typ, "( u8 , u8 )");
        assert_eq!(unwrap_progress(p).extent(), (0, 11))
    }

    #[test]
    fn type_with_generics() {
        let p = qp(typ, "A<T>");
        assert_eq!(unwrap_progress(p).extent(), (0, 4))
    }

    #[test]
    fn type_with_generics_all_space() {
        let p = qp(typ, "A < T >");
        assert_eq!(unwrap_progress(p).extent(), (0, 7))
    }

    #[test]
    fn type_impl_trait() {
        let p = qp(typ, "impl Foo");
        assert_eq!(unwrap_progress(p).extent(), (0, 8))
    }

    #[test]
    fn type_fn_trait() {
        let p = qp(typ, "Fn(u8) -> u8");
        assert_eq!(unwrap_progress(p).extent(), (0, 12))
    }

    #[test]
    fn type_ref() {
        let p = qp(typ, "&mut Foo");
        assert_eq!(unwrap_progress(p).extent(), (0, 8))
    }

    #[test]
    fn type_mut_ref() {
        let p = qp(typ, "&mut Foo");
        assert_eq!(unwrap_progress(p).extent(), (0, 8))
    }

    #[test]
    fn type_mut_ref_with_lifetime() {
        let p = qp(typ, "&'a mut Foo");
        assert_eq!(unwrap_progress(p).extent(), (0, 11))
    }

    #[test]
    fn type_const_pointer() {
        let p = qp(typ, "*const Foo");
        assert_eq!(unwrap_progress(p).extent(), (0, 10))
    }

    #[test]
    fn type_mut_pointer() {
        let p = qp(typ, "*mut Foo");
        assert_eq!(unwrap_progress(p).extent(), (0, 8))
    }

    #[test]
    fn type_uninhabited() {
        let p = qp(typ, "!");
        assert_eq!(unwrap_progress(p).extent(), (0, 1))
    }

    #[test]
    fn type_slice() {
        let p = qp(typ, "[u8]");
        assert_eq!(unwrap_progress(p).extent(), (0, 4))
    }

    #[test]
    fn type_array() {
        let p = qp(typ, "[u8; 42]");
        assert_eq!(unwrap_progress(p).extent(), (0, 8))
    }

    #[test]
    fn type_array_allows_expressions() {
        let p = qp(typ, "[u8; 1 + 1]");
        assert_eq!(unwrap_progress(p).extent(), (0, 11))
    }

    #[test]
    fn type_fn() {
        let p = qp(typ, "fn(u8) -> u8");
        assert_eq!(unwrap_progress(p).extent(), (0, 12))
    }

    #[test]
    fn type_fn_with_names() {
        let p = qp(typ, "fn(a: u8) -> u8");
        assert_eq!(unwrap_progress(p).extent(), (0, 15))
    }

    #[test]
    fn type_fn_with_const() {
        let p = qp(typ, "const fn()");
        assert_eq!(unwrap_progress(p).extent(), (0, 10))
    }

    #[test]
    fn type_fn_with_unsafe() {
        let p = qp(typ, "unsafe fn()");
        assert_eq!(unwrap_progress(p).extent(), (0, 11))
    }

    #[test]
    fn type_fn_with_extern() {
        let p = qp(typ, "extern fn()");
        assert_eq!(unwrap_progress(p).extent(), (0, 11))
    }

    #[test]
    fn type_fn_with_extern_and_abi() {
        let p = qp(typ, r#"extern "C" fn()"#);
        assert_eq!(unwrap_progress(p).extent(), (0, 15))
    }

    #[test]
    fn type_higher_ranked_trait_bounds() {
        let p = qp(typ, "for <'a> Foo<'a>");
        assert_eq!(unwrap_progress(p).extent(), (0, 16))
    }

    #[test]
    fn type_higher_ranked_trait_bounds_on_functions() {
        let p = qp(typ, "for <'a> fn(&'a u8)");
        assert_eq!(unwrap_progress(p).extent(), (0, 19))
    }

    #[test]
    fn type_higher_ranked_trait_bounds_on_references() {
        let p = qp(typ, "for <'a> &'a u8");
        assert_eq!(unwrap_progress(p).extent(), (0, 15))
    }

    #[test]
    fn type_combination() {
        let p = qp(typ, "Foo + Bar");
        assert_eq!(unwrap_progress(p).extent(), (0, 9))
    }

    #[test]
    fn type_combination_with_lifetimes() {
        let p = qp(typ, "Foo + 'a");
        assert_eq!(unwrap_progress(p).extent(), (0, 8))
    }

    #[test]
    fn type_combination_of_higher_ranked_trait_bounds_and_lifetimes() {
        let p = qp(typ, "for<'a> Foo + 'a");
        assert_eq!(unwrap_progress(p).extent(), (0, 16))
    }

    #[test]
    fn type_combination_of_impl_trait_and_lifetimes() {
        let p = qp(typ, "impl Foo + 'a");
        assert_eq!(unwrap_progress(p).extent(), (0, 13))
    }

    #[test]
    fn type_disambiguation() {
        let p = qp(typ, "<Foo as Bar>");
        assert_eq!(unwrap_progress(p).extent(), (0, 12))
    }

    #[test]
    fn type_disambiguation_with_associated_type() {
        let p = qp(typ, "<Foo as Bar>::Quux");
        assert_eq!(unwrap_progress(p).extent(), (0, 18))
    }

    #[test]
    fn type_disambiguation_without_disambiguation() {
        let p = qp(typ, "<Foo>");
        assert_eq!(unwrap_progress(p).extent(), (0, 5))
    }

    #[test]
    fn struct_basic() {
        let p = qp(p_struct, "struct S { field: TheType, other: OtherType }");
        assert_eq!(unwrap_progress(p).extent, (0, 45))
    }

    #[test]
    fn struct_with_generic_fields() {
        let p = qp(p_struct, "struct S { field: Option<u8> }");
        assert_eq!(unwrap_progress(p).extent, (0, 30))
    }

    #[test]
    fn struct_with_fields_with_no_space() {
        let p = qp(p_struct, "struct S{a:u8}");
        assert_eq!(unwrap_progress(p).extent, (0, 14))
    }

    #[test]
    fn struct_with_fields_with_all_space() {
        let p = qp(p_struct, "struct S { a : u8 }");
        assert_eq!(unwrap_progress(p).extent, (0, 19))
    }

    #[test]
    fn struct_with_generic_declarations() {
        let p = qp(p_struct, "struct S<T> { field: Option<T> }");
        assert_eq!(unwrap_progress(p).extent, (0, 32))
    }

    #[test]
    fn struct_public() {
        let p = qp(p_struct, "pub struct S {}");
        assert_eq!(unwrap_progress(p).extent, (0, 15))
    }

    #[test]
    fn struct_public_field() {
        let p = qp(p_struct, "struct S { pub age: u8 }");
        assert_eq!(unwrap_progress(p).extent, (0, 24))
    }

    #[test]
    fn struct_with_attributed_field() {
        let p = qp(p_struct, "struct S { #[foo(bar)] #[baz(quux)] field: u8 }");
        assert_eq!(unwrap_progress(p).extent, (0, 47))
    }

    #[test]
    fn struct_with_tuple() {
        let p = qp(p_struct, "struct S(u8);");
        assert_eq!(unwrap_progress(p).extent, (0, 13))
    }

    #[test]
    fn struct_with_tuple_and_annotation() {
        let p = qp(p_struct, "struct S(#[foo] u8);");
        assert_eq!(unwrap_progress(p).extent, (0, 20))
    }

    #[test]
    fn struct_with_tuple_and_visibility() {
        let p = qp(p_struct, "struct S(pub u8);");
        assert_eq!(unwrap_progress(p).extent, (0, 17))
    }

    #[test]
    fn struct_empty() {
        let p = qp(p_struct, "struct S;");
        assert_eq!(unwrap_progress(p).extent, (0, 9))
    }

    #[test]
    fn struct_with_where_clause() {
        let p = qp(p_struct, "struct S<A> where A: Foo { a: A }");
        assert_eq!(unwrap_progress(p).extent, (0, 33))
    }

    #[test]
    fn struct_with_tuple_and_where_clause() {
        let p = qp(p_struct, "struct S<A>(A) where A: Foo;");
        assert_eq!(unwrap_progress(p).extent, (0, 28))
    }

    #[test]
    fn where_clause_with_path() {
        let p = qp(where_clause_item, "P: foo::bar::baz::Quux<'a>");
        assert_eq!(unwrap_progress(p).extent(), (0, 26))
    }

    #[test]
    fn where_clause_with_multiple_bounds() {
        let p = qp(where_clause_item, "P: A + B");
        assert_eq!(unwrap_progress(p).extent(), (0, 8))
    }

    #[test]
    fn where_clause_with_multiple_types() {
        let p = qp(where_clause, "where P: A, Q: B");
        let (p, _) = unwrap_progress(p);
        assert_eq!(p[1].extent(), (12, 16))
    }

    #[test]
    fn where_clause_with_lifetimes() {
        let p = qp(where_clause_item, "'a: 'b + 'c");
        assert_eq!(unwrap_progress(p).extent(), (0, 11))
    }

    #[test]
    fn comment_end_of_line() {
        let p = qp(comment, "// hello");
        assert_eq!(unwrap_progress(p).extent, (0, 8))
    }

    #[test]
    fn comment_region() {
        let p = qp(comment, "/* hello */");
        assert_eq!(unwrap_progress(p).extent, (0, 11))
    }

    #[test]
    fn ident_with_leading_underscore() {
        let p = qp(ident, "_foo");
        assert_eq!(unwrap_progress(p).extent, (0, 4))
    }

    #[test]
    fn ident_can_not_be_keyword() {
        let p = qp(ident, "for");
        assert_eq!(unwrap_progress_err(p), (0, vec![Error::ExpectedIdentifier]))
    }

    #[test]
    fn ident_can_have_keyword_substring() {
        let p = qp(ident, "form");
        assert_eq!(unwrap_progress(p).extent, (0, 4))
    }

    #[test]
    fn lifetime_ident() {
        let p = qp(lifetime, "'a");
        assert_eq!(unwrap_progress(p).extent, (0, 2))
    }

    #[test]
    fn lifetime_static() {
        let p = qp(lifetime, "'static");
        assert_eq!(unwrap_progress(p).extent, (0, 7))
    }

    #[test]
    fn generic_declarations_() {
        let p = qp(generic_declarations, "<A>");
        assert_eq!(unwrap_progress(p).extent, (0, 3))
    }

    #[test]
    fn generic_declarations_allow_type_bounds() {
        let p = qp(generic_declarations, "<A: Foo>");
        assert_eq!(unwrap_progress(p).extent, (0, 8))
    }

    #[test]
    fn generic_declarations_with_default_types() {
        let p = qp(generic_declarations, "<A = Bar>");
        assert_eq!(unwrap_progress(p).extent, (0, 9))
    }

    #[test]
    fn generic_declarations_allow_lifetime_bounds() {
        let p = qp(generic_declarations, "<'a: 'b>");
        assert_eq!(unwrap_progress(p).extent, (0, 8))
    }

    #[test]
    fn generic_declarations_with_attributes() {
        let p = qp(generic_declarations, "<#[foo] 'a, #[bar] B>");
        assert_eq!(unwrap_progress(p).extent, (0, 21))
    }

    #[test]
    fn generic_declarations_all_space() {
        let p = qp(generic_declarations, "< 'a : 'b , A : Foo >");
        assert_eq!(unwrap_progress(p).extent, (0, 21))
    }

    #[test]
    fn trait_bounds_with_lifetime() {
        let p = qp(trait_bounds, "'a + 'b");
        assert_eq!(unwrap_progress(p).extent, (0, 7))
    }

    #[test]
    fn trait_bounds_with_relaxed() {
        let p = qp(trait_bounds, "?A + ?B");
        assert_eq!(unwrap_progress(p).extent, (0, 7))
    }

    #[test]
    fn trait_bounds_with_associated_types() {
        let p = qp(trait_bounds, "A<B, C = D>");
        assert_eq!(unwrap_progress(p).extent, (0, 11))
    }

    #[test]
    fn visibility_self() {
        let p = qp(visibility, "pub(self)");
        assert_eq!(unwrap_progress(p).extent, (0, 9))
    }

    #[test]
    fn visibility_super() {
        let p = qp(visibility, "pub(super)");
        assert_eq!(unwrap_progress(p).extent, (0, 10))
    }

    #[test]
    fn visibility_crate() {
        let p = qp(visibility, "pub(crate)");
        assert_eq!(unwrap_progress(p).extent, (0, 10))
    }

    #[test]
    fn visibility_path() {
        let p = qp(visibility, "pub(::foo::bar)");
        assert_eq!(unwrap_progress(p).extent, (0, 15))
    }

    #[test]
    fn zero_or_more_tailed_with_zero() {
        let p = qp(zero_or_more_tailed(",", literal("X")), "");
        let p = unwrap_progress(p);
        assert_eq!(p.values.len(), 0);
        assert_eq!(p.separator_count, 0);
    }

    #[test]
    fn zero_or_more_tailed_with_one() {
        let p = qp(zero_or_more_tailed(",", literal("X")), "X");
        let p = unwrap_progress(p);
        assert_eq!(p.values.len(), 1);
        assert_eq!(p.separator_count, 0);
    }

    #[test]
    fn zero_or_more_tailed_with_one_trailing() {
        let p = qp(zero_or_more_tailed(",", literal("X")), "X,");
        let p = unwrap_progress(p);
        assert_eq!(p.values.len(), 1);
        assert_eq!(p.separator_count, 1);
    }

    #[test]
    fn zero_or_more_tailed_with_two() {
        let p = qp(zero_or_more_tailed(",", literal("X")), "X, X");
        let p = unwrap_progress(p);
        assert_eq!(p.values.len(), 2);
        assert_eq!(p.separator_count, 1);
    }

    #[test]
    fn zero_or_more_tailed_with_two_trailing() {
        let p = qp(zero_or_more_tailed(",", literal("X")), "X, X,");
        let p = unwrap_progress(p);
        assert_eq!(p.values.len(), 2);
        assert_eq!(p.separator_count, 2);
    }

    #[test]
    fn zero_or_more_tailed_with_all_space() {
        let p = qp(zero_or_more_tailed(",", literal("X")), "X , X , ");
        let p = unwrap_progress(p);
        assert_eq!(p.values.len(), 2);
        assert_eq!(p.separator_count, 2);
    }

    #[test]
    fn zero_or_more_tailed_doesnt_allow_space_separator() {
        let p = qp(zero_or_more_tailed(",", literal("X")), "X X");
        let p = unwrap_progress(p);
        assert_eq!(p.values.len(), 1);
        assert_eq!(p.separator_count, 0);
    }

    #[test]
    fn one_or_more_tailed_with_zero() {
        let p = qp(one_or_more_tailed(",", literal("X")), "");
        let (err_loc, errs) = unwrap_progress_err(p);
        assert_eq!(err_loc, 0);
        assert!(errs.contains(&Error::Literal("X")));
    }

    #[test]
    fn one_or_more_tailed_with_one() {
        let p = qp(one_or_more_tailed(",", literal("X")), "X");
        let p = unwrap_progress(p);
        assert_eq!(p.values.len(), 1);
        assert_eq!(p.separator_count, 0);
    }

    #[test]
    fn one_or_more_tailed_with_one_trailing() {
        let p = qp(one_or_more_tailed(",", literal("X")), "X,");
        let p = unwrap_progress(p);
        assert_eq!(p.values.len(), 1);
        assert_eq!(p.separator_count, 1);
    }

    #[test]
    fn one_or_more_tailed_with_two() {
        let p = qp(one_or_more_tailed(",", literal("X")), "X, X");
        let p = unwrap_progress(p);
        assert_eq!(p.values.len(), 2);
        assert_eq!(p.separator_count, 1);
    }

    #[test]
    fn one_or_more_tailed_with_two_trailing() {
        let p = qp(one_or_more_tailed(",", literal("X")), "X, X,");
        let p = unwrap_progress(p);
        assert_eq!(p.values.len(), 2);
        assert_eq!(p.separator_count, 2);
    }

    #[test]
    fn one_or_more_tailed_with_all_space() {
        let p = qp(one_or_more_tailed(",", literal("X")), "X , X , ");
        let p = unwrap_progress(p);
        assert_eq!(p.values.len(), 2);
        assert_eq!(p.separator_count, 2);
    }

    #[test]
    fn one_or_more_tailed_with_two_doesnt_allow_space_separator() {
        let p = qp(one_or_more_tailed(",", literal("X")), "X X");
        let p = unwrap_progress(p);
        assert_eq!(p.values.len(), 1);
        assert_eq!(p.separator_count, 0);
    }

    fn unwrap_progress<P, T, E>(p: peresil::Progress<P, T, E>) -> T
        where P: std::fmt::Debug,
              E: std::fmt::Debug,
    {
        match p {
            peresil::Progress { status: peresil::Status::Success(v), .. } => v,
            peresil::Progress { status: peresil::Status::Failure(e), point } => {
                panic!("Failed parsing at {:?}: {:?}", point, e)
            }
        }
    }

    fn unwrap_progress_err_generic<P, T, E>(p: peresil::Progress<P, T, E>) -> (P, E) {
        match p {
            peresil::Progress { status: peresil::Status::Success(_), .. } => {
                panic!("Parsing should have failed, but it did not");
            }
            peresil::Progress { status: peresil::Status::Failure(e), point } => {
                (point, e)
            }
        }
    }

    fn unwrap_progress_err<T, E>(p: peresil::Progress<peresil::StringPoint, T, E>) -> (usize, E) {
        let (pt, e) = unwrap_progress_err_generic(p);
        (pt.offset, e)
    }
}
