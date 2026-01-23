# Contributing to PrintBridge

Thank you for your interest in contributing to PrintBridge! This document provides guidelines and information for contributors.

## Ways to Contribute

### Bug Reports

- Search existing issues first to avoid duplicates
- Use the bug report template
- Include reproduction steps, expected behavior, and actual behavior
- Include system information (OS version, browser, printer model)

### Feature Requests

- Describe the use case and problem you're trying to solve
- Explain why existing features don't meet your needs
- Consider if the feature fits the project's scope

### Code Contributions

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/your-feature`)
3. Make your changes
4. Run tests (`cargo test` for agent, `npm test` for SDK)
5. Commit with clear messages
6. Push to your fork
7. Open a Pull Request

### Documentation

- Fix typos and improve clarity
- Add examples and use cases
- Translate documentation

### Testing

- Test on different printers and report compatibility
- Test edge cases and report issues
- Help improve test coverage

## Development Setup

### Prerequisites

- **Rust:** 1.70 or later
- **Node.js:** 18 or later
- **Windows:** 10/11 with Visual Studio Build Tools (for agent development)

### Agent (Rust/Tauri)

```bash
cd agent
cargo build
cargo test
cargo run  # Run in development mode
```

### SDK (TypeScript)

```bash
cd sdk
npm install
npm run build
npm test
npm run lint
```

## Code Style

### Rust

- Run `cargo fmt` before committing
- Run `cargo clippy` and address warnings
- Follow Rust naming conventions
- Document public APIs with doc comments

### TypeScript

- Run `npm run lint` before committing
- Use TypeScript strict mode
- Export types for public APIs
- Write JSDoc comments for public methods

## Commit Messages

Use conventional commits format:

```
type(scope): description

[optional body]

[optional footer]
```

Types:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation only
- `style`: Formatting, no code change
- `refactor`: Code restructuring
- `test`: Adding tests
- `chore`: Maintenance tasks

Examples:
```
feat(agent): add support for ZPL label printing
fix(sdk): handle reconnection timeout correctly
docs: add examples for batch printing
```

## Pull Request Guidelines

- One feature or fix per PR
- Link related issues
- Update documentation if needed
- Add tests for new functionality
- Ensure all tests pass
- Keep changes focused and minimal

## Testing Guidelines

### Agent Tests

- Unit tests for individual modules
- Integration tests for print workflows
- Mock printer interactions where possible

### SDK Tests

- Unit tests for all public methods
- Mock WebSocket for connection tests
- Test error handling paths

## Architecture Decisions

Major changes should be discussed in an issue first. Consider:

- Does this fit the project's goals?
- Is this the simplest solution?
- Does this maintain backward compatibility?
- What are the performance implications?

## Getting Help

- Read existing documentation
- Search closed issues
- Ask in GitHub Discussions
- Open a new issue with detailed context

## Code of Conduct

- Be respectful and constructive
- Welcome newcomers
- Focus on the technical merits
- Assume good intentions

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
