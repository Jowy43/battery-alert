# Arquitectura

Battery Alert es una aplicación nativa de bandeja para Windows. Está escrita en Rust 2021 y usa `windows-sys` para llamar a las API Win32; no incorpora un runtime gráfico web.

## Componentes

- `src/main.rs`: punto de entrada y manejo del error de inicio.
- `src/tray.rs`: ventana oculta, icono de notificación, menú, temporizador, avisos de batería baja y opción de inicio con Windows.
- `src/hid.rs`: enumeración de dispositivos HID, comunicación con los informes de Logitech y conversión de lecturas a porcentajes.
- `Cargo.toml` y `Cargo.lock`: dependencias y configuración de compilación.
- `.github/workflows/release.yml`: compilación de Windows y publicación de artefactos al enviar una etiqueta de versión.

## Ciclo de lectura

1. Windows enumera las interfaces HID presentes mediante SetupAPI.
2. Battery Alert filtra las interfaces del fabricante Logitech y los identificadores contemplados en `src/hid.rs`.
3. La aplicación abre la interfaz y consulta informes de batería del protocolo HID Logitech. Para los receptores de ratón intenta localizar el dispositivo conectado.
4. Se prefiere un porcentaje informado por el periférico. Si las respuestas disponibles contienen voltaje, se aplica una curva aproximada y la interfaz antepone `~`.
5. La bandeja mantiene un icono por dispositivo: la silueta de un ratón y la de unos cascos, cada una con una franja coloreada según su propio nivel. Si no hay lectura disponible, la franja queda marcada como no disponible. El clic izquierdo abre un panel con los dos dispositivos, porcentajes, barras de nivel y controles; el clic derecho abre el menú tradicional.

La consulta se inicia al arrancar y se repite cada 60 segundos. El comando **Actualizar ahora** ejecuta el mismo ciclo antes del siguiente intervalo.

## Interfaz y avisos

El proceso crea una ventana Win32 oculta para recibir eventos de los iconos y del temporizador. Cada icono tiene niveles gráficos en tramos de diez puntos y la descripción emergente muestra el porcentaje de ese dispositivo. El panel se dibuja con GDI, se ancla junto al cursor y ofrece actualización manual y el control de inicio con Windows. El menú de clic derecho muestra ambos porcentajes y el voltaje leído cuando el porcentaje es estimado. Las estimaciones se marcan con `~`.

El aviso de batería baja se dispara al detectar un valor de 20 % o menos. Para evitar avisos repetidos, ese dispositivo vuelve a quedar habilitado para notificar cuando la lectura llega al 26 % o más. Este estado solo vive mientras la aplicación está abierta.

La opción **Iniciar con Windows** crea o elimina el valor `BatteryAlert` en `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`. Solo modifica la configuración del usuario actual y no requiere instalar un servicio.

## Estados sin porcentaje

La aplicación distingue entre dispositivo no detectado, interfaz que no se puede abrir y dispositivo conectado que no responde. Un dispositivo dormido o una interfaz ocupada por otro software puede no responder. En esos casos la aplicación deja la lectura sin porcentaje, en vez de presentar como actual un valor almacenado.

La comunicación HID se hace desde el ciclo de refresco de la bandeja. El tiempo de respuesta depende del periférico, el receptor y las interfaces que exponga Windows.

## Límites actuales

- La búsqueda de dispositivos y los protocolos de batería están orientados a los modelos e identificadores Logitech descritos en el README; no es un monitor genérico de cualquier periférico Bluetooth o USB.
- El porcentaje derivado de voltaje es una estimación, no una lectura calibrada por Logitech.
- No se muestra el estado de carga ni un historial de batería.
- La lectura real depende de que firmware, receptor y estado de energía respondan a los informes HID.
- La aplicación no incluye instalador, firma de código ni actualizador automático.
