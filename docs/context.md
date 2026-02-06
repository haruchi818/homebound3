# Project Context

## Stack
- Frontend: Svelte.
- Backend: Rust with Actix.
- Database PostgreSQL

## UI Style
- Material Design 3 (Material You).
- Prefer surfaceContainer backgrounds.
- Avoid card-heavy layouts.
- Favor spacing over borders.
- Expressive but minimal.

## Screens and Behavior

### Home / Login
- Home page is a login screen.
- Authentication is Google-only via OAuth2 from Google Cloud.

### Dashboard (After Login)
- Layout uses the full window.
- Top bar: 15% of window height.
  - Right side shows the user avatar.
  - Clicking the avatar opens a menu with a Logout button.
- Main section: 75% of window height.
  - Area to add applications.
  - Looks like a desktop (Android or Windows style) with clickable icons.
- Bottom bar: 10% of window height.
  - Contains Settings and Chat buttons.
  - Clicking Chat opens a right-side panel listing all logged-in users.
  - Clicking a user opens a chat view similar to Facebook chat for messaging.

## Navigation and Routing
- Unauthenticated users are redirected to the Home / Login screen.
- Authenticated users land on the Dashboard.
- Logout returns the user to the Home / Login screen.

## Authentication and Session
- Google OAuth2 is the only login method.
- Store minimal user profile data needed for avatar and display name.
- Sessions should expire cleanly and require re-authentication.

## Presence and Realtime
- The chat sidebar shows currently logged-in users.
- Presence updates in near real time.
- If a user goes offline, they disappear from the list without reload.

## Chat Behavior
- Opening chat does not navigate away from the dashboard.
- Chat is a right-side panel that reduces main content width.
- Conversation list and active chat are visible within the panel.
- Messages show sender, timestamp, and delivery status.

## Settings
- Settings opens from the bottom bar.
- Settings should avoid heavy cards and use spacing-driven sections.

## Responsive Behavior
- Layout proportions (15/75/10) scale with window height.
- Chat panel adapts to narrow screens by overlaying instead of shrinking.

## Accessibility
- All interactive elements are keyboard reachable.
- Color contrast meets Material Design 3 accessibility guidance.

## Non-Functional Requirements
- Fast first render on login.
- Smooth transitions when opening menus and chat.
- Avoid visual clutter; prioritize whitespace and hierarchy.
