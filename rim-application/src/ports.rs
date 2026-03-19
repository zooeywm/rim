/// Char-offset edit operation for swap log replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwapEditOp {
	Insert { pos: usize, text: String },
	Delete { pos: usize, len: usize },
}
