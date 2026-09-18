from pathlib import Path
import hashlib
import re


def blob_sha(text):
    data = text.encode()
    return hashlib.sha1(f'blob {len(data)}\0'.encode() + data).hexdigest()


def replace_once(text, old, new):
    assert text.count(old) == 1, (old[:120], text.count(old))
    return text.replace(old, new, 1)


def take_section(text, start, end):
    a = text.index(start)
    b = text.index(end, a)
    return text[:a] + text[b:], text[a:b]


def take_function(text, signature):
    a = text.index(signature)
    opening = text.index('{', a)
    depth = 0
    for i in range(opening, len(text)):
        if text[i] == '{':
            depth += 1
        elif text[i] == '}':
            depth -= 1
            if depth == 0:
                return text[:a] + text[i+1:], text[a:i+1]
    raise AssertionError('unterminated function')


session_path = Path('crates/noon/src/execution_session.rs')
session = session_path.read_text()
assert blob_sha(session) == '327d5818326bafc30891789750c0fe75347eb1e3'
integration_path = Path('crates/noon/src/integration.rs')
integration = integration_path.read_text()
assert blob_sha(integration) == 'ee20bca0f990c0512bcb36cb29b80d70113e3f26'

session, errors = take_section(session,
    '/// Error produced when semantic/native reactive input',
    '/// Error produced when the canonical semantic camera')
errors = replace_once(errors,
    '    RequiredCallbackPending,',
    '''    PointerNotConfigured,
    ForeignPointerRuntime,
    StalePointerBinding,
    PointerContextMismatch,
    StalePointerPublication { expected: PublicationContext, actual: PublicationContext },
    WrongPointer { expected: NativePointerId, actual: NativePointerId },
    PointerBindingSequenceExhausted,
    ContextualPointerRequired,
    RequiredCallbackPending,''')
errors = replace_once(errors,
    '        match self {',
    '''        match self {
            Self::PointerNotConfigured => formatter.write_str("contextual pointer input is not configured"),
            Self::ForeignPointerRuntime => formatter.write_str("pointer token belongs to another runtime incarnation"),
            Self::StalePointerBinding => formatter.write_str("pointer binding has been replaced"),
            Self::PointerContextMismatch => formatter.write_str("pointer record does not match its captured view/publication context"),
            Self::StalePointerPublication { expected, actual } => write!(formatter, "pointer publication {actual:?} is not the current publication {expected:?}"),
            Self::WrongPointer { expected, actual } => write!(formatter, "pointer {actual:?} is not the configured pointer {expected:?}"),
            Self::PointerBindingSequenceExhausted => formatter.write_str("pointer binding sequence is exhausted"),
            Self::ContextualPointerRequired => formatter.write_str("configured pointer input requires an occurrence-local context"),''')

session, _native = take_section(session,
    '    /// Deliver one normalized sampled native state source',
    '    /// Resolve an authoritative semantic object identity')
session, batch = take_function(session, '    fn apply_reactive_input_batch(')
batch = replace_once(batch, '    fn apply_reactive_input_batch(', '    pub(super) fn apply_reactive_input_batch(')
session, conversion = take_function(session, 'fn reactive_value_from_native(')
session = replace_once(session, 'const NATIVE_EVENT_SEQUENCE_WRAP: f32 = 1_000_000.0;\n', '')
session = replace_once(session, 'mod publication;', '''mod input;
pub use input::{ExecutionSessionInputError, NativePointerInputPublication, NativePointerInputToken};
mod publication;''')
session = replace_once(session, '    last_native_event_sequence: Option<u64>,', '    last_native_event_sequence: Option<u64>,\n    pointer_input: input::PointerInputState,')
session = replace_once(session, '            last_native_event_sequence: self.last_native_event_sequence,', '            last_native_event_sequence: self.last_native_event_sequence,\n            pointer_input: self.pointer_input.clone(),')
session = replace_once(session, '            last_native_event_sequence: None,', '            last_native_event_sequence: None,\n            pointer_input: input::PointerInputState::default(),')
session = replace_once(session, '''    AnimationOptions, Camera2DState, NativeEventOccurrence, NativeInputRuntimeError,
    NativeInputValue, NativeStateSource, NativeStateUpdate, ObjectId, RateFunction, ReactiveError,''', '''    AnimationOptions, Camera2DState, ObjectId, RateFunction, ReactiveError,''')
# Preserve all existing library tests; only restore their now-local native imports.
start = session.index('#[cfg(test)]\nmod tests {')
production, tests = session[:start], session[start:]
imports = re.search(r'    use noon_core::\{(.*?)\n    \};', tests, re.S)
assert imports
native_names = ['NativeEventOccurrence', 'NativeInputRuntimeError', 'NativeInputValue', 'NativeStateSource', 'NativeStateUpdate']
missing = [name for name in native_names if re.search(r'\b' + name + r'\b', tests) and not re.search(r'\b' + name + r'\b', imports.group(1))]
if missing:
    tests = tests[:imports.start(1)] + '\n        ' + ', '.join(missing) + ',' + tests[imports.start(1):]
if 'NATIVE_EVENT_SEQUENCE_WRAP' in tests:
    tests = replace_once(tests, '    use super::*;', '    use super::*;\n    use super::input::NATIVE_EVENT_SEQUENCE_WRAP;')
session = production + tests
for name in native_names:
    assert not re.search(r'\b' + name + r'\b', production), name
session_path.write_text(session)

module_path = Path('crates/noon/src/execution_session/input.rs')
module = module_path.read_text()
module = replace_once(module, '// INPUT_ERROR_DEFINITION', errors.rstrip())
module = replace_once(module, '    // INPUT_BATCH_DEFINITION', batch.rstrip())
module = replace_once(module, '// NATIVE_VALUE_CONVERSION', conversion.rstrip())
module = module.replace('                update.source,', '                &update.source,')
module = module.replace('                occurrence.source,', '                &occurrence.source,')
module_path.write_text(module)

integration = replace_once(integration, 'pub use crate::execution_segment::{ExecutionSegmentSequence, ExecutionSegmentToken};', '''pub use crate::execution_segment::{ExecutionSegmentSequence, ExecutionSegmentToken};
pub use crate::execution_session::{NativePointerInputPublication, NativePointerInputToken};
pub use noon_core::{
    NativeInputModifiers, NativePointerCancellation, NativePointerContext, NativePointerId,
    NativePointerInput, NativePointerInputKind, NativePointerPosition,
};''')
integration_path.write_text(integration)
print('Assembled scoped session module; preserved original non-input code and library tests.')
