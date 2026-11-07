// Brücke zwischen der Rust-Oberfläche und Leaflet (Standortkarte).
// Leaflet selbst liegt unter vendor/leaflet (BSD-2-Clause) und wird vor diesem Skript geladen.
(function () {
  var map = null;
  var layer = null;
  var onSelect = null;

  var COLORS = { normal: "#3d8a62", warning: "#c08417", critical: "#c84d38", offline: "#8f877f" };

  window.bbMap = {
    // Karte im Element mit der Kennung `id` aufbauen. `select` wird mit der Gerätekennung aufgerufen.
    init: function (id, select) {
      this.destroy();
      var el = document.getElementById(id);
      if (!el || !window.L) return false;
      onSelect = select;
      map = L.map(el, { zoomControl: true }).setView([48.78, 9.12], 11);
      L.tileLayer("https://tile.openstreetmap.org/{z}/{x}/{y}.png", {
        maxZoom: 19,
        attribution: '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>-Mitwirkende',
      }).addTo(map);
      layer = L.layerGroup().addTo(map);
      return true;
    },

    // Marker setzen. `devices` ist eine Liste von {id, label, status, lat, lon}.
    // `fit` passt den Kartenausschnitt an alle Marker an.
    setMarkers: function (devices, fit) {
      if (!map || !layer) return;
      layer.clearLayers();
      var bounds = [];
      devices.forEach(function (d) {
        var color = COLORS[d.status] || COLORS.offline;
        // Erfundene Geräte (Simulator) sind als gestrichelter Ring dargestellt, echte als gefüllter Punkt.
        var invented = d.source === "simulation";
        var marker = L.circleMarker([d.lat, d.lon], {
          radius: d.status === "critical" ? 9 : 7,
          color: invented ? color : "#ffffff",
          weight: invented ? 2 : 1.5,
          dashArray: invented ? "3 3" : null,
          fillColor: color,
          fillOpacity: invented ? 0.25 : 0.95,
          className: d.pulse ? "bb-pulse" : "",
        });
        marker.bindTooltip(d.label, { direction: "top" });
        marker.on("click", function () {
          if (onSelect) onSelect(d.id);
        });
        marker.addTo(layer);
        bounds.push([d.lat, d.lon]);
      });
      if (fit && bounds.length) map.fitBounds(bounds, { padding: [40, 40], maxZoom: 15 });
    },

    invalidate: function () {
      if (map) map.invalidateSize();
    },

    destroy: function () {
      if (map) {
        map.remove();
        map = null;
        layer = null;
        onSelect = null;
      }
    },
  };
})();
