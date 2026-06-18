# TOWER HACKER — Guía de pasos (qué haces TÚ)

> Yo (Claude) ya escribí **todo el código**. Este archivo lista solo lo que
> tienes que ejecutar tú, en orden, para compilar el juego a **WebAssembly** y
> abrirlo en el navegador. Cada paso dice exactamente qué copiar y qué deberías
> ver. Si algo falla, ve a la sección **"Si algo sale mal"** al final.

---

## 0. Contexto en 30 segundos

- El juego está escrito en **Rust** + **macroquad** y compila a un solo archivo
  `.wasm` que corre en cualquier navegador (PC y móvil), a 60fps.
- Es **100% autocontenido / limpio legalmente**: sin Google Fonts, sin CDN de
  React, sin llamadas de red en tiempo de ejecución, sin marcas registradas.
  La única dependencia es `macroquad` (licencia MIT / Apache-2.0).
- Carpeta del proyecto: `D:\atari\tower-hacker`

Archivos que ya dejé listos:

```
tower-hacker\
├── Cargo.toml            <- config del proyecto Rust
├── src\main.rs           <- el juego completo
├── index.html            <- "cascarón" que carga el .wasm
├── mq_js_bundle.js       <- pegamento JS de macroquad (MIT, ya descargado)
├── build.ps1             <- compila y copia el .wasm  (lo corres tú)
├── serve.py              <- servidor local para probar  (lo corres tú)
└── PASOS.md              <- este archivo
```

---

## ⚠️ ANTES DE EMPEZAR — Cortex XDR

Al instalar Rust, **Cortex XDR marcó el instalador** (`rustup-init.exe`) con la
heurística `script_dropper`. Fue un **falso positivo**: rustup descarga y "suelta"
archivos, y esa conducta dispara la regla. La instalación terminó bien.

El siguiente paso (`cargo build`) **descarga ~100 librerías y compila miles de
archivos**, así que **puede volver a disparar la misma alerta de XDR.**

Tienes 3 opciones (elige una):

1. **(Recomendado) Pide a TI una excepción** para estas dos rutas, o agrégalas tú
   si tienes permiso en la consola de XDR:
   - `%USERPROFILE%\.cargo`
   - `D:\atari\tower-hacker`
2. **Corre el build igual** y, si XDR bloquea, manda la captura a TI para que lo
   permitan (es desarrollo legítimo, no malware).
3. Si no puedes compilar aquí, compílalo en **otra máquina sin XDR** o en CI; el
   código es portable.

> No voy a intentar evadir ni desactivar XDR. Eso lo decides tú / TI.

---

## 1. Verificar que Rust responde  ✅ (rápido, NO dispara XDR)

En esta sesión de Claude, escribe en el prompt (con el `!` al inicio):

```
! & "$env:USERPROFILE\.cargo\bin\rustc.exe" --version
```

Deberías ver algo como `rustc 1.96.0 (...)`. Si lo ves, sigue al paso 2.

---

## 1.5. Instalar la variante GNU de Rust  🧩 (UNA sola vez — PUEDE tocar XDR)

El Rust por defecto (MSVC) necesita el linker de Visual Studio (`link.exe`), que
no está instalado. En vez de instalar Visual Studio (varios GB), usamos la
variante **GNU**, que trae su propio linker. Córrelo una vez:

```
! & "$env:USERPROFILE\.cargo\bin\rustup.exe" toolchain install stable-x86_64-pc-windows-gnu
```
```
! & "$env:USERPROFILE\.cargo\bin\rustup.exe" target add wasm32-unknown-unknown --toolchain stable-x86_64-pc-windows-gnu
```

Cuando ambos terminen sin error, sigue al paso 2.

---

## 2. Compilar el juego a WASM  🔨 (este paso PUEDE tocar XDR — ver aviso arriba)

Desde `D:\atari\tower-hacker`. La forma más fácil — corre el script que ya te dejé:

```
! powershell -ExecutionPolicy Bypass -File "D:\atari\tower-hacker\build.ps1"
```

Si prefieres el comando crudo (hace lo mismo):

```
! & "$env:USERPROFILE\.cargo\bin\cargo.exe" build --release --target wasm32-unknown-unknown --manifest-path "D:\atari\tower-hacker\Cargo.toml"
```
…y luego copia el resultado al lado del index.html:
```
! Copy-Item "D:\atari\tower-hacker\target\wasm32-unknown-unknown\release\towerhacker.wasm" "D:\atari\tower-hacker\towerhacker.wasm" -Force
```

**Qué esperar:**
- La **primera** compilación tarda varios minutos (descarga y compila macroquad).
  Las siguientes son rápidas.
- Al final debe decir `Finished` y que copió `towerhacker.wasm`.
- ❗ **Si la compilación tira errores rojos**, NO te preocupes: cópialos y
  pégamelos en el chat. Yo escribí el código sin poder compilarlo aquí (por XDR),
  así que es posible que haya 1–2 errores de tipos que yo corrijo al instante.
  **Pégame el texto completo del error** y lo arreglo.

---

## 3. Probar en el navegador  ▶️ (NO dispara XDR — solo sirve archivos locales)

```
! python "D:\atari\tower-hacker\serve.py"
```

Verás:
```
Tower Hacker  ->  http://localhost:8080
```

Abre **http://localhost:8080** en tu navegador. Para detener el servidor: `Ctrl+C`.

> Nota: hay que abrirlo por `http://`, **no** haciendo doble-clic al `index.html`
> (los navegadores no cargan `.wasm` desde `file://`).

**Móvil:** estando el PC y el celular en la misma red Wi-Fi, abre en el celular
`http://LA-IP-DE-TU-PC:8080` (saca la IP con `! ipconfig`). Gira a horizontal.

---

## 4. Jugar

- **Toca/clic en un nodo vacío** → aparece un menú radial con torres → toca para
  colocar. **Toca una torre puesta** → menú para mejorar o vender.
- Botón **INICIAR OLEADA** abajo al centro. **II** pausa. **1x/2x** acelera.
- Arriba a la derecha: **EN/ES** (idioma) y **CRT** (filtro de scanlines).

---

## Qué cambió respecto al prototipo original (mejoras que pediste)

- **Aleatoriedad:** oleadas con variación (conteos/tiempos), enemigos **ÉLITE**
  aleatorios (más vida/velocidad, recompensa doble), **golpes críticos** (x2.2),
  casillas de **anomalía** aleatorias por partida, lluvia de glifos aleatoria.
- **Más color:** rift dimensional con **ciclo de tono (HSV)**, jefe arcoíris,
  paletas más ricas, partículas y camino neón.
- **Más dificultad:** escalado de vida más agresivo, élites, y eventos de
  **CAMBIO DIMENSIONAL** periódicos que aceleran/potencian a todos los virus.
- **Multidimensionalidad:** fondo en **3 capas de parallax** a distintas
  profundidades + una "dimensión" global que recolorea todo el tablero.
- **Legal:** sin fuentes/CDN externos, sin red en runtime, sin marcas; todo con
  licencias permisivas (MIT/Apache).

---

## Si algo sale mal

| Síntoma | Qué hacer |
|---|---|
| `rustc`/`cargo` "no se reconoce" | Cierra y reabre la terminal, o usa la ruta completa `"$env:USERPROFILE\.cargo\bin\cargo.exe"`. |
| Errores **rojos** al compilar | Cópialos y **pégamelos en el chat** — los corrijo. |
| XDR bloquea el build | Captura la alerta y mándala a TI, o pide excepción para `%USERPROFILE%\.cargo` y la carpeta del proyecto. |
| Pantalla en blanco en el navegador | Asegúrate de abrir por `http://localhost:8080` (no `file://`) y de que `towerhacker.wasm` esté junto a `index.html`. Mira la consola del navegador (F12). |
| `.wasm` no encontrado al servir | Faltó el paso 2 de copiar el `.wasm`. Re-corre `build.ps1`. |
| Cambié el código y no se ve | Re-corre el paso 2 (recompila) y recarga el navegador con Ctrl+F5. |

---

## 👉 Dónde vamos ahora

**Tu próximo paso es el PASO 2 (compilar).**
Cuando lo corras:
- Si dice **`Finished`** → ve al PASO 3 y juega. 🎉
- Si salen **errores** → pégamelos aquí y los arreglo enseguida.

Estoy listo para iterar en cuanto me pases la salida del compilador.
