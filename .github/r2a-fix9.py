"""Final typed consumer fixture correction and redundant identity forwarding."""
from pathlib import Path
import re

path=Path('../tooling/.github/r2a-tests/object_authoring_errors.rs')
s=path.read_text()
before='''    assert!(matches!(
        family.arrange(f64::NAN, 0.0, 0.1, true),
        Err(AuthoringError::InvalidRenderNumber { .. })
    ));'''
after='''    let direction_error = family.arrange(f64::NAN, 0.0, 0.1, true).unwrap_err();
    assert!(matches!(
        direction_error,
        AuthoringError::VectorLowering(
            noon::integration::SemanticLoweringError::NonFiniteVector(_)
        )
    ), "unexpected typed direction error: {direction_error:?}");
    assert!(direction_error.source().unwrap().is::<noon::integration::SemanticLoweringError>());'''
assert s.count(before)==1,s.count(before)
path.write_text(s.replace(before,after))

# validate_content already returns AuthoringError, not a lower-level cause.
path=Path('crates/noon/src/semantic_mobject.rs')
s=path.read_text()
s,count=re.subn(r'(validate_content\([^;\n]*\))\.map_err\(AuthoringError::from\)\?',r'\1?',s)
assert count==4,count
path.write_text(s)
