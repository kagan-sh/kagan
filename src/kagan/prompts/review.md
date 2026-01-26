# Code Review Request

## Ticket: {title}

**ID:** {ticket_id}
**Description:** {description}

## Changes Made

### Commits
{commits}

### Diff Summary
{diff_summary}

## Review Criteria

Please review the changes against:
1. Does the implementation match the ticket description?
2. Are there any obvious bugs or issues?
3. Is the code reasonably clean and maintainable?

## Your Task

1. Review the changes
2. Provide a brief summary of what was implemented
3. End with exactly ONE signal:

- `<approve summary="Brief 1-2 sentence summary of work done"/>` - Changes are good
- `<reject reason="What needs to be fixed"/>` - Changes need work

Example:
```
The implementation looks good. Added the new feature with proper error handling.
<approve summary="Implemented user authentication with JWT tokens and proper validation"/>
```
