# Preparar y publicar una release

El repositorio incluye un flujo de GitHub Actions en `.github/workflows/release.yml`. Al subir una etiqueta cuyo nombre empieza por `v`, compila la aplicación en Windows, empaqueta el ejecutable y crea una release en GitHub con el ZIP y su suma SHA-256.

## Primera publicación

1. Crea un repositorio vacío en GitHub y configura su URL como remoto `origin`.
2. Sube la rama principal:

   ```powershell
   git push -u origin main
   ```

3. Comprueba que GitHub Actions está habilitado en el repositorio.
4. Crea y sube la etiqueta inicial. Debe coincidir con la versión de `Cargo.toml` (`0.1.0` en la versión inicial):

   ```powershell
   git tag -a v0.1.0 -m "Battery Alert v0.1.0"
   git push origin v0.1.0
   ```

5. Revisa la ejecución del flujo **Release** en la pestaña **Actions**. Cuando termine correctamente, la release y los archivos estarán en **Releases**.

## Publicaciones posteriores

1. Actualiza `version` en `Cargo.toml` y actualiza el paquete raíz de `Cargo.lock` con una compilación Cargo.
2. Confirma los cambios y sube la rama.
3. Crea una etiqueta nueva, por ejemplo `v0.2.0`, sobre el commit que quieras publicar y súbela:

   ```powershell
   git tag -a v0.2.0 -m "Battery Alert v0.2.0"
   git push origin v0.2.0
   ```

El flujo usa el token de GitHub Actions del propio repositorio con permiso de escritura de contenido para crear la release. Si una compilación falla, la etiqueta no producirá una release completa; revisa el registro del flujo y vuelve a ejecutarlo después de corregir el problema.

Cada paquete publicado contiene `battery-alert.exe` y `README.md`. También se adjunta un archivo SHA-256 para comprobar la integridad del ZIP. Los artefactos no están firmados y no incluyen instalador.
