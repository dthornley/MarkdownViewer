## 1.6.2

- Fixed: while typing a search, a match that stopped matching could keep its highlight (for example the orange current-match colour on a dash after `-` became `-1`) until something else repainted that part of the page. This is a WebKit repaint bug in the system web view; the viewer now updates its highlights in a way that avoids it.
