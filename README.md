# Battery Alert

Aplicación ligera para Windows que consulta la batería de algunos periféricos Logitech y muestra el nivel en el área de notificación, junto al reloj.

## Funciones

- Consulta el **Logitech G502 LIGHTSPEED** y los **Logitech PRO X Wireless**.
- Actualiza las lecturas cada minuto; el menú permite pedir una actualización inmediata.
- Muestra un icono independiente para el ratón y otro para los cascos, cada uno con su propio indicador de batería. El porcentaje numérico aparece al pasar el puntero sobre cada icono.
- Al hacer clic izquierdo en cualquiera de los iconos, abre un panel visual con ambos dispositivos, sus porcentajes, barras de batería y controles. El clic derecho conserva el menú de la aplicación.
- Avisa con una notificación de Windows al llegar al 20 % o menos. No repite el aviso hasta que la batería sube al 26 %.
- Puede iniciarse con Windows, opción que se activa desde el menú del icono.
- No necesita Logitech G HUB abierto.

## Compatibilidad y precisión

El lector busca interfaces HID Logitech con estos identificadores de producto:

| Dispositivo | Identificador |
| --- | --- |
| G502 LIGHTSPEED | `046D:C08D` |
| PRO X Wireless | `046D:0ABA` |

También contempla varios identificadores de receptores LIGHTSPEED para ratones. Que un identificador esté contemplado no garantiza una lectura con todos los firmwares, receptores o configuraciones. La lectura de estos dos periféricos debe confirmarse en el equipo donde se ejecute la aplicación.

Cuando el dispositivo proporciona un porcentaje, se muestra directamente. Si solo responde con voltaje, Battery Alert calcula una estimación y añade `~` al porcentaje. El G502 LIGHTSPEED comunica el voltaje, así que su porcentaje es aproximado y puede diferir de G HUB. El menú muestra también el voltaje leído para facilitar la comparación. Un dispositivo dormido, desconectado o con su interfaz HID ocupada puede aparecer sin lectura; en ese caso no se conserva como actual un valor anterior.

## Descargar y ejecutar

La página **Releases** publicará un ZIP con el ejecutable y su suma SHA-256 cuando se publique una etiqueta de versión `v*` en GitHub. Extrae el ZIP y ejecuta `battery-alert.exe`. Es una aplicación portable: no incluye instalador ni desinstalador.

Los iconos pueden quedar en el menú de iconos ocultos de Windows. Cada uno representa el tramo de batería de su dispositivo; al dejar el puntero sobre el ratón o los cascos se ve su porcentaje correspondiente. El panel se cierra al hacer clic fuera o en el botón de cierre.

## Requisitos

- Windows 10 u 11 de 64 bits.
- Para compilar: Rust y Cargo instalados en Windows.

## Compilar desde el código

```powershell
cargo build --release --locked
```

El ejecutable se genera en `target\release\battery-alert.exe`. Para iniciarlo desde PowerShell:

```powershell
.\target\release\battery-alert.exe
```

## Documentación

- [Arquitectura y funcionamiento](docs/arquitectura.md)
- [Preparar y publicar una release](docs/lanzamientos.md)

## Licencia

La licencia del proyecto está pendiente de decisión. No se incluye una licencia ni se atribuye una licencia en `Cargo.toml`.
