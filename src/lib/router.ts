export type Route = "home" | "video" | "comic" | "game" | "normalize" | "settings";
type Handler = (route: Route) => void;

class Router {
  private handlers: Handler[] = [];
  current: Route = "home";
  on(h: Handler) { this.handlers.push(h); }
  go(route: Route) { this.current = route; this.handlers.forEach(h => h(route)); }
}
export const router = new Router();
