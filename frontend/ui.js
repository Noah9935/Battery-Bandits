// Kleine Brücke für Seitenübergänge (View Transitions API). Rust ruft `bbUi.transition(run)` auf.
// Ohne Unterstützung im Browser oder bei "Bewegung reduzieren" läuft `run` einfach sofort.
(function () {
  window.bbUi = {
    transition: function (run) {
      var reduce = window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
      if (!document.startViewTransition || reduce) {
        run();
        return;
      }
      document.startViewTransition(function () {
        run();
        // Die Oberfläche zeichnet nach dem Setzen der Signale erst im nächsten Schritt neu: zwei Bilder abwarten.
        return new Promise(function (resolve) {
          requestAnimationFrame(function () {
            requestAnimationFrame(resolve);
          });
        });
      });
    },
  };
})();
