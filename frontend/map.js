// Kartengrundlagen: eine saubere Straßenkarte (hell oder dunkel, ohne Höhenmarken) und Luftbild (Satellit).
window.bbBase = {
  isDark: function () {
    var theme = document.documentElement.dataset.theme;
    if (theme) return theme === "dark";
    return !!(window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches);
  },
  // Liefert die Ebenen einer Grundlage (bei der dunklen Karte Grund und Beschriftung getrennt).
  // Esri-Kacheln brauchen keinen Schlüssel. Die Standardkarte von OpenStreetMap zeigt Gipfel als Dreiecke, diese hier nicht.
  create: function (kind, dark) {
    var esri = "https://server.arcgisonline.com/ArcGIS/rest/services/";
    if (kind === "sat") {
      return [
        L.tileLayer(esri + "World_Imagery/MapServer/tile/{z}/{y}/{x}", {
          maxZoom: 19,
          attribution: "Luftbild &copy; Esri, Maxar, Earthstar Geographics",
        }),
      ];
    }
    // "Schwarz-Weiß": ruhige Flächen in Grau und Weiß ohne Orange, damit die bunten Markierungen herausstechen.
    if (kind === "simple" && !dark) {
      return [
        L.tileLayer(esri + "Canvas/World_Light_Gray_Base/MapServer/tile/{z}/{y}/{x}", {
          maxZoom: 19,
          maxNativeZoom: 16,
          attribution: "Karte &copy; Esri, HERE, Garmin, OpenStreetMap-Mitwirkende",
        }),
        L.tileLayer(esri + "Canvas/World_Light_Gray_Reference/MapServer/tile/{z}/{y}/{x}", {
          maxZoom: 19,
          maxNativeZoom: 16,
        }),
      ];
    }
    if (dark) {
      return [
        L.tileLayer(esri + "Canvas/World_Dark_Gray_Base/MapServer/tile/{z}/{y}/{x}", {
          maxZoom: 19,
          maxNativeZoom: 16,
          attribution: "Karte &copy; Esri, HERE, Garmin, OpenStreetMap-Mitwirkende",
        }),
        L.tileLayer(esri + "Canvas/World_Dark_Gray_Reference/MapServer/tile/{z}/{y}/{x}", {
          maxZoom: 19,
          maxNativeZoom: 16,
        }),
      ];
    }
    return [
      L.tileLayer(esri + "World_Street_Map/MapServer/tile/{z}/{y}/{x}", {
        maxZoom: 19,
        attribution: "Karte &copy; Esri, HERE, Garmin, OpenStreetMap-Mitwirkende",
      }),
    ];
  },
};

// Ruhiges Gleiten einer waagerechten Leiste (Ticker). Der Browser scrollt mit "smooth" immer sehr schnell, deshalb eigene
// Animation mit weichem Anfang und Ende. Während des Gleitens ist das Einrasten aus, danach rastet die Leiste wieder ein.
window.bbUi = {
  slide: function (el, to, ms) {
    if (!el) return;
    var from = el.scrollLeft;
    var dist = to - from;
    if (Math.abs(dist) < 1) return;
    var token = (el._bbSlide = (el._bbSlide || 0) + 1);
    el.style.scrollSnapType = "none";
    el.style.scrollBehavior = "auto";
    var start = null;
    function ease(t) {
      return t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2;
    }
    function done() {
      el.style.scrollSnapType = "";
      el.style.scrollBehavior = "";
    }
    if (!(ms > 0)) {
      el.scrollLeft = to;
      return done();
    }
    function frame(ts) {
      if (el._bbSlide !== token) return; // abgebrochen oder von einem neueren Gleiten abgelöst
      if (start === null) start = ts;
      var t = Math.min(1, (ts - start) / ms);
      el.scrollLeft = from + dist * ease(t);
      if (t < 1) requestAnimationFrame(frame);
      else done();
    }
    requestAnimationFrame(frame);
  },
  cancel: function (el) {
    if (!el) return;
    el._bbSlide = (el._bbSlide || 0) + 1;
    el.style.scrollSnapType = "";
    el.style.scrollBehavior = "";
  },
};

// Zweite, kleine Karte zum Auswählen eines Ortes (Optionen): Ein Tipp setzt die Markierung.
(function () {
  var map = null;
  var marker = null;
  // Eigene Markierung als Punkt (die Standardgrafik von Leaflet liegt nicht im Paket und zeigte nur "Marker").
  function pin(latlng) {
    return L.marker(latlng, {
      keyboard: false,
      icon: L.divIcon({ className: "bb-pick-pin", html: "<span></span>", iconSize: [26, 26], iconAnchor: [13, 13] }),
    });
  }
  window.bbPick = {
    init: function (id, lat, lon, has, onPick) {
      this.destroy();
      var el = document.getElementById(id);
      if (!el || !window.L) return false;
      map = L.map(el).setView(has ? [lat, lon] : [48.78, 9.18], has ? 14 : 10);
      window.bbBase.create("map", window.bbBase.isDark()).forEach(function (l) {
        l.addTo(map);
      });
      if (has) marker = pin([lat, lon]).addTo(map);
      map.on("click", function (e) {
        if (marker) marker.setLatLng(e.latlng);
        else marker = pin(e.latlng).addTo(map);
        onPick(e.latlng.lat, e.latlng.lng);
      });
      return true;
    },
    set: function (lat, lon) {
      if (!map) return;
      if (marker) marker.setLatLng([lat, lon]);
      else marker = pin([lat, lon]).addTo(map);
      map.setView([lat, lon], Math.max(map.getZoom(), 14));
    },
    clear: function () {
      if (marker && map) map.removeLayer(marker);
      marker = null;
    },
    invalidate: function () {
      if (map) map.invalidateSize();
    },
    destroy: function () {
      if (map) {
        map.remove();
        map = null;
        marker = null;
      }
    },
  };
})();

// Brücke zwischen der Rust-Oberfläche und Leaflet (Standortkarte).
// Leaflet selbst liegt unter vendor/leaflet (BSD-2-Clause) und wird vor diesem Skript geladen.
(function () {
  var map = null;
  var layer = null;
  var onSelect = null;
  var markersById = {};
  var pinned = {};
  var meDot = null;
  var meRing = null;
  var baseLayers = [];
  var baseKind = "map";
  var baseDark = false;

  var COLORS = { normal: "#34c759", warning: "#ff9500", critical: "#ff3b30", offline: "#8e8e93" };

  // Angepinnte Geräte: größer, mit auffälligem Rand in der Markenfarbe.
  function stylePin(marker, id) {
    if (pinned[id]) {
      marker.setStyle({ radius: 11, weight: 4, color: "#b4661c" });
      marker.bringToFront();
    } else {
      marker.setStyle({ radius: 7, weight: 1.5, color: "#ffffff" });
    }
  }

  window.bbMap = {
    // Eigener Standort: blauer Punkt mit hellem Ring, damit er nie mit einem Gerät verwechselt wird
    // (Geräte sind grün, orange, rot oder grau). Mit `center` fliegt die Karte dorthin.
    setMe: function (lat, lon, center) {
      if (!map) return;
      var at = [lat, lon];
      if (!meDot) {
        meRing = L.circleMarker(at, { radius: 18, stroke: false, fillColor: "#0a84ff", fillOpacity: 0.2, interactive: false }).addTo(map);
        meDot = L.circleMarker(at, { radius: 8, color: "#ffffff", weight: 3, fillColor: "#0a84ff", fillOpacity: 1, interactive: false }).addTo(map);
        meDot.bindTooltip("Du bist hier", { direction: "top", offset: [0, -8] });
      } else {
        meRing.setLatLng(at);
        meDot.setLatLng(at);
      }
      meDot.bringToFront();
      if (center) map.flyTo(at, Math.max(map.getZoom(), 14), { duration: 0.8 });
    },
    // Kennungen der angepinnten Geräte setzen und die Marker neu einfärben.
    setPins: function (ids) {
      pinned = {};
      (ids || []).forEach(function (id) { pinned[id] = true; });
      Object.keys(markersById).forEach(function (id) { stylePin(markersById[id], id); });
    },
    // Kartengrundlage wechseln: "map" (Straßenkarte), "simple" (einfache Grautöne) oder "sat" (Luftbild),
    // Karte und Grautöne passend zum hellen oder dunklen Farbschema.
    setBase: function (kind, dark) {
      if (!map) return;
      var next = kind === "sat" ? "sat" : kind === "simple" ? "simple" : "map";
      if (baseLayers.length && baseKind === next && (next === "sat" || baseDark === !!dark)) return;
      baseLayers.forEach(function (l) {
        map.removeLayer(l);
      });
      baseKind = next;
      baseDark = !!dark;
      baseLayers = window.bbBase.create(next, baseDark);
      baseLayers.forEach(function (l) {
        l.addTo(map);
      });
      // Beschriftung zuerst nach hinten, dann der Grund darunter: So liegt die Beschriftung über dem Grund.
      baseLayers
        .slice()
        .reverse()
        .forEach(function (l) {
          l.bringToBack();
        });
    },

    // Karte im Element mit der Kennung `id` aufbauen. `select` wird mit der Gerätekennung aufgerufen.
    init: function (id, select) {
      this.destroy();
      var el = document.getElementById(id);
      if (!el || !window.L) return false;
      onSelect = select;
      map = L.map(el, { zoomControl: false }).setView([48.78, 9.12], 11);
      L.control.zoom({ position: "bottomright" }).addTo(map);
      baseKind = "map";
      baseDark = window.bbBase.isDark();
      baseLayers = window.bbBase.create(baseKind, baseDark);
      baseLayers.forEach(function (l) {
        l.addTo(map);
      });
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
        stylePin(marker, d.id);
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
        markersById = {};
        meDot = null;
        meRing = null;
      }
    },
  };
})();
