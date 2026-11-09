# Battery Alert

Aplicación ligera para Windows que detecta periféricos HID que publican su nivel de batería y lo muestra en la bandeja del sistema. Cada dispositivo tiene su propio icono, con una figura según su tipo y una franja de batería.

## Compatibilidad

Battery Alert combina dos métodos de lectura:

- **HID estándar:** busca el uso `Battery Strength` definido en HID Usage Page `0x06` y lee los informes de entrada o de características que ofrece el periférico. El valor se convierte a porcentaje usando el rango lógico declarado por el dispositivo.
- **Logitech HID++:** mantiene un adaptador específico para Logitech G502 LIGHTSPEED, PRO X Wireless y los identificadores de receptores LIGHTSPEED contemplados por el lector. Cuando Logitech solo comunica voltaje, el porcentaje se estima y se marca con `~`.

La detección es dinámica: no hay una lista fija de dos tarjetas. Los dispositivos estándar compatibles se muestran con el nombre que declara su interfaz HID y se clasifican como ratón, cascos, teclado, mando o dispositivo genérico.

**No existe una lectura universal para todos los periféricos.** Muchos fabricantes no publican `Battery Strength` y usan protocolos privados. Esos modelos requieren un adaptador compatible; si Windows o el dispositivo no exponen una lectura admitida, Battery Alert no puede inventar el porcentaje y no lo mostrará como una batería válida. Un dispositivo dormido, desconectado o con la interfaz ocupada puede dejar de responder hasta la siguiente actualización.

## Funciones

- Actualiza los niveles cada minuto y permite solicitar una actualización inmediata.
- Muestra un icono independiente por dispositivo, con su tipo y batería. El porcentaje aparece al pasar el puntero sobre el icono.
- Al hacer clic en cualquier icono, abre un panel con las lecturas disponibles. Si hay más de dos dispositivos, se puede recorrer la lista con la rueda del ratón.
- Avisa con una notificación de Windows al llegar al 20 % o menos. No repite el aviso hasta que la batería sube al 26 %.
- Puede iniciarse con Windows; la opción está en el menú del icono.
- No necesita Logitech G HUB abierto para consultar el adaptador Logitech.
- No envía datos ni consulta un servicio en Internet.

Windows puede colocar algunos iconos en el menú de iconos ocultos. Se puede elegir qué iconos aparecen siempre desde la configuración de la bandeja del sistema.

## Descargar y ejecutar

La página **Releases** publica un ZIP con el ejecutable y su suma SHA-256 al crear una etiqueta `v*` en GitHub. Extrae el ZIP y ejecuta `battery-alert.exe`. Es una aplicación portable: no incluye instalador ni desinstalador.

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
