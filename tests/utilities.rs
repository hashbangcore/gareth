use gareth::core::utilities;

const LOREM_MD: &str = r#"
# Lorem Ipsum

## Intro
Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.

## Section 1
- **Duis aute** irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur.
- **Excepteur sint** occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.

## Section 2
"Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua." - *Cita ficticia*

```python
def gareth():
    print("end")
```
"#;

#[test]
fn test_render_markdown_markdown_mode() {
    use utilities::{RenderMode, render};

    let result = render(LOREM_MD, RenderMode::Markdown);
    assert!(result.contains("Intro"));
    assert!(result.contains("end"));
}

#[test]
fn test_capitalize() {
    use utilities::capitalize;

    let output = capitalize("rust");
    assert_eq!(output, "Rust");

    let output = capitalize("");
    assert_eq!(output, "");

    let output = capitalize("a");
    assert_eq!(output, "A");
}
