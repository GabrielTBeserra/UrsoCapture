# UrsoCapture

Aplicação desktop de alta performance construída com Tauri 2, Vue 3 e Rust.

## 🚀 Como funciona o CI/CD (GitHub Actions)

O projeto está configurado com um fluxo automatizado em [`.github/workflows/build-and-release.yml`](.github/workflows/build-and-release.yml):

### 1. Geração de Artefatos em Modificações (Push / Pull Request)
- A cada `push` nas branches `main` ou `master` (ou `pull_request`), o workflow executa a compilação cruzada para **Windows**, **Linux** e **macOS**.
- Os instaladores gerados ficam disponíveis para download diretamente na aba **Actions** do GitHub, na seção **Artifacts** de cada execução (`UrsoCapture-windows-installer`, etc.).

### 2. Publicação Automática de Releases (Tags de Versão)
- Quando uma tag com prefixo `v` for criada e enviada (ex: `v0.1.0`), o GitHub Actions criará automaticamente uma **GitHub Release** oficial com todos os executáveis e instaladores anexados.

```bash
# Exemplo para gerar uma nova versão oficial:
git tag v0.1.0
git push origin v0.1.0
```

### 3. Disparo Manual (Workflow Dispatch)
- Você também pode disparar a compilação e download dos instaladores manualmente a qualquer momento pela aba **Actions** > **Build and Release Installers** > **Run workflow**.

---

## 🛠️ Desenvolvimento Local

```bash
# Instalar dependências
yarn install

# Rodar em modo de desenvolvimento (Vite + Tauri)
yarn tauri dev

# Gerar build do instalador localmente
yarn tauri build
```

