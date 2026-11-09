# Arquitectura

Battery Alert es una aplicación nativa de bandeja para Windows. Está escrita en Rust 2021 y usa `windows-sys` para llamar a las API Win32; no incorpora un runtime gráfico web.

## Componentes

- `src/main.rs`: punto de entrada y manejo del error de inicio.
- `src/tray.rs`: ventana oculta, icono de notificación, menú, temporizador, avisos de batería baja y opción de inicio con Windows.
- `src/hid.rs`: enumeración HID, lectura del uso estándar `Battery Strength`, adaptador Logitech HID++ y conversión de lecturas a porcentajes.
- `Cargo.toml` y `Cargo.lock`: dependencias y configuración de compilación.
- `.github/workflows/release.yml`: compilación de Windows y publicación de artefactos al enviar una etiqueta de versión.

## Ciclo de lectura

1. Windows enumera las interfaces HID presentes mediante SetupAPI.
2. Para periféricos que declaran el uso HID estándar `Battery Strength` (usage page `0x06`, usage `0x20`), la aplicación consulta los informes de entrada y de características y normaliza el valor con los límites lógicos del descriptor.
3. Las interfaces Logitech de los modelos e identificadores admitidos se consultan además mediante el protocolo HID++ específico. Para receptores LIGHTSPEED de ratón intenta localizar los dispositivos conectados.
4. Se prefiere el porcentaje comunicado por el periférico. Si Logitech solo responde con voltaje, se aplica una curva aproximada y la interfaz antepone `~`.
5. La bandeja crea un icono independiente por cada dispositivo leído. La silueta se clasifica con la colección HID o el nombre declarado; hay iconos para ratón, cascos, teclado, mando y dispositivo genérico. El clic izquierdo abre un panel dinámico con dos tarjetas visibles y desplazamiento con la rueda si hay más dispositivos.

La consulta se inicia al arrancar y se repite cada 60 segundos. El comando **Actualizar ahora** ejecuta el mismo ciclo antes del siguiente intervalo.

## Interfaz y avisos

El proceso crea una ventana Win32 oculta para recibir eventos de los iconos y del temporizador. Cada icono tiene una franja coloreada según la lectura y la descripción emergente muestra el nombre y porcentaje de ese dispositivo. El panel se dibuja con GDI, se ancla junto al cursor y ofrece actualización manual y el control de inicio con Windows. El menú de clic derecho muestra los porcentajes leídos y el voltaje cuando Logitech solo informa voltaje. Las estimaciones se marcan con `~`.

El aviso de batería baja se dispara al detectar un valor de 20 % o menos. Para evitar avisos repetidos, ese dispositivo vuelve a quedar habilitado para notificar cuando la lectura llega al 26 % o más. Este estado solo vive mientras la aplicación está abierta.

La opción **Iniciar con Windows** crea o elimina el valor `BatteryAlert` en `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`. Solo modifica la configuración del usuario actual y no requiere instalar un servicio.

## Estados sin porcentaje

La aplicación distingue entre dispositivo no detectado, interfaz que no se puede abrir y dispositivo conectado que no responde. Un dispositivo dormido o una interfaz ocupada por otro software puede no responder. En esos casos la aplicación deja la lectura sin porcentaje, en vez de presentar como actual un valor almacenado.

La comunicación HID se hace desde el ciclo de refresco de la bandeja. El tiempo de respuesta depende del periférico, el receptor y las interfaces que exponga Windows.

## Límites actuales

- La capa genérica solo puede leer periféricos HID que publiquen el uso estándar `Battery Strength`. Los fabricantes que usen protocolos privados requieren un adaptador; Logitech HID++ está implementado para los modelos e identificadores indicados en el README.
- El nombre y el tipo dependen de la información que publique el descriptor HID. Algunos periféricos pueden aparecer con un nombre genérico o una silueta aproximada.
- El porcentaje derivado de voltaje es una estimación, no una lectura calibrada por Logitech.
- No se muestra el estado de carga ni un historial de batería.
- La lectura real depende de que firmware, receptor y estado de energía respondan a los informes HID.
- La aplicación no incluye instalador, firma de código ni actualizador automático.
