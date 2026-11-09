// Brücke zwischen der Rust-Oberfläche und Leaflet (Standortkarte).
// Leaflet selbst liegt unter vendor/leaflet (BSD-2-Clause) und wird vor diesem Skript geladen.
(function () {
  var map = null;
  var layer = null;
  var onSelect = null;
  var markersById = {};

  var COLORS = { normal: "#34c759", warning: "#ff9500", critical: "#ff3b30", offline: "#8e8e93" };

  window.bbMap = {
    // Karte im Element mit der Kennung `id` aufbauen. `select` wird mit der Gerätekennung aufgerufen.
    init: function (id, select) {
      this.destroy();
      var el = document.getElementById(id);
      if (!el || !window.L) return false;
      onSelect = select;
      map = L.map(el, { zoomControl: false }).setView([48.78, 9.12], 11);
      L.control.zoom({ position: "bottomright" }).addTo(map);
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
      markersById = {};
      var bounds = [];
      devices.forEach(function (d) {
        var color = COLORS[d.status] || COLORS.offline;
        // Ruhige Punkte in der Farbe des Status. Erfundene Geräte (Simulator) sind etwas blasser.
        var invented = d.source === "simulation";
        var marker = L.circleMarker([d.lat, d.lon], {
          radius: 7,
          color: "#ffffff",
          weight: 1.5,
          fillColor: color,
          fillOpacity: invented ? 0.5 : 0.95,
          // Offline: gestrichelter Rand, wie in der Tabelle
          dashArray: d.status === "offline" ? "3 2" : null,
        });
        marker.bindTooltip(d.label, { direction: "top" });
        marker.on("click", function () {
          if (onSelect) onSelect(d.id);
        });
        marker.addTo(layer);
        markersById[d.id] = marker;
        bounds.push([d.lat, d.lon]);
      });
      if (fit && bounds.length) map.fitBounds(bounds, { padding: [40, 40], maxZoom: 15 });
    },

    // Springt zum Gerät: sanft heranzoomen und den Namen zeigen.
    focus: function (id) {
      var marker = markersById[id];
      if (!map || !marker) return false;
      map.flyTo(marker.getLatLng(), Math.max(map.getZoom(), 14), { duration: 0.8 });
      marker.openTooltip();
      return true;
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
