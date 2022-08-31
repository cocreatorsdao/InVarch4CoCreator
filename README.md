# git-remote-inv4
A Git helper that integrates INV4 with the Git protocol.

## Installing
Make sure you already have `cargo` and `rust` installed. Then:
```sh
cargo install --git https://github.com/InvArch/INV4-Git
```
The binary will be installed at `~/.cargo/bin/` as `git-remote-inv4`

## Testing
Testing requires running an IPFS node, running a local InvArch node and creating an IP Set on it.

### Running the local IPFS node:
Install the ipfs cli binary from a package manager, or follow the instructions on the [IPFS documentation](http://docs.ipfs.tech.ipns.localhost:8080/install/command-line/#linux) to get the binary installed in your system.

After you installed the IPFS cli tool, open a terminal and run the following commands:
```sh
ipfs init
ipfs daemon
```
Now set that terminal aside, it will run the IPFS node until you manually kill it or close the terminal.


### Running the local InvArch node:
On a new terminal, run the following:
```sh
git clone https://github.com/InvArch/InvArch-Node
cd InvArch-Node
make build
make run-solo-alice
```

Now that you have built the node binary and that terminal is running a collator, open a second terminal on the same location, and run another collator with the following command:
```sh
make run-solo-bob
```

A development node will start running and should be accessible from `ws://127.0.0.1:9944` (or directly in polkadot.js at https://polkadot.js.org/?rpc=ws://127.0.0.1:9944)
You can now set these terminals aside too.

### Sending tokens to your account
You're gonna need an account for which you have the seed phrase on hand, you can create a new account for this.
To send tokens to that account, follow these steps:
1. Go to this page in polkadot.js: https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A9944#/accounts
2. On the tabs, hover over the accounts tab and click transfer, as shown in the image: ![](images/tabs.png)
3. Select Alice as the sender account in the first field.
4. Paste your account in the second field and an arbitrary amount in the third field, 100 in the case of the example: ![](images/transfer.png)
5. Click the make transfer button and confirm the transaction.

### Creating the IP Set
1. Open the pre-made extrinsic in polkadot.js using the following URL: https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A9944#/extrinsics/decode/0x470038676974207265706f7369746f72790000010132010000
2. Go to the submission tab: ![](images/submission_tab.png)
3. Submit the transaction using the account that you sent tokens to.

An IP Set should have been created with the ID `0`

### Using the tool
Using a terminal, navigate to a directory where you'll create a new git repository and run the following commands (considering you're on Linux):
 ```sh
mkdir test-repo
cd test-repo
touch test-file
echo test > test-file
git init
git remote add origin inv4://0
git add *
git commit -m "First commit!"
```

This next command will send your files to the chain, it will ask for your seed phrase, just paste it in the terminal and press Enter:
```sh
git push origin master
```

Now you have created a new local git repository, added some files, linked to the IPS you created on-chain and pushed your local commit to the chain!

To demonstrate that it really is on-chain, go to a new directory and clone the git repo from the chain using the following command:
```sh
git clone inv4://0 cloned
```

Now you can navigate inside this cloned repo and verify that it's the same as the one you pushed!

##Español
# git-remote-inv4
Un ayudante de Git que integra INV4 con el protocolo Git.

## Instalación
Asegúrate de que ya tienes instalados `cargo` y `rust`. Entonces:
``sh
cargo install --git https://github.com/InvArch/INV4-Git
```
El binario se instalará en `~/.cargo/bin/` como `git-remote-inv4`.

## Pruebas
Las pruebas requieren ejecutar un nodo IPFS, ejecutar un nodo InvArch local y crear un IP Set en él.

### Ejecutando el nodo IPFS local:
Instala el binario ipfs cli desde un gestor de paquetes, o sigue las instrucciones de la [documentación de IPFS]([http://docs.ipfs.tech.ipns.localhost:8080/install/command-line/#linux](https://docs.ipfs.tech/install/command-line/#official-distributions)) para conseguir instalar el binario en tu sistema.

Después de instalar la herramienta IPFS cli, abre un terminal y ejecuta los siguientes comandos:
``sh
ipfs init
ipfs daemon
```
Ahora aparta ese terminal, ejecutará el nodo IPFS hasta que lo mates manualmente o cierres el terminal.

Para poder ejecutar la construcción del nodo necesitaras tener instaladas las herramientas de wasm lo puedes hacer ubicándote en el path de la aplicación y corriendo el siguiente comando:
```
rustup target add wasm32-unknown-unknown
```

### Ejecutando el nodo local InvArch:
En una nueva terminal, ejecute lo siguiente:
``sh
git clone https://github.com/InvArch/InvArch-Node
cd InvArch-Node
make build
make run-solo-alice
```

Ahora que has construido el binario del nodo y esa terminal está ejecutando un collator, abre una segunda terminal en la misma ubicación, y ejecuta otro collator con el siguiente comando
``sh
make run-solo-bob
```

Un nodo de desarrollo comenzará a ejecutarse y debería ser accesible desde `ws://127.0.0.1:9944` (o directamente en polkadot.js en https://polkadot.js.org/?rpc=ws://127.0.0.1:9944)
Ahora también puedes poner estos terminales a un lado.

### Envío de tokens a tu cuenta
Vas a necesitar una cuenta para la cual tengas la frase semilla a mano, puedes crear una nueva cuenta para esto.
Para enviar tokens a esa cuenta, sigue estos pasos:
1. Ve a esta página en polkadot.js: https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A9944#/accounts
2. En las pestañas, pasa el ratón por encima de la pestaña de cuentas y haz clic en transferir, como se muestra en la imagen: ![](images/tabs.png)
3. Selecciona Alice como cuenta remitente en el primer campo.
4. Pega tu cuenta en el segundo campo y una cantidad arbitraria en el tercero, 100 en el caso del ejemplo: ![](images/transfer.png)
5. Haga clic en el botón de realizar la transferencia y confirme la transacción.

### Creación del conjunto de IP
1. Abrir la extrínseca prefabricada en polkadot.js utilizando la siguiente URL: https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A9944#/extrinsics/decode/0x470038676974207265706f7369746f72790000010132010000
2. Vaya a la pestaña de presentación: ![](images/submission_tab.png)
3. Envíe la transacción utilizando la cuenta a la que envió los tokens.

Se debería haber creado un conjunto de IP con el ID `0`.

### Usando la herramienta
Usando un terminal, navega a un directorio donde crearás un nuevo repositorio git y ejecuta los siguientes comandos (considerando que estás en Linux)
 ``sh
mkdir test-repo
cd test-repo
touch archivo-prueba
echo test > test-file
git init
git remote add origen inv4://0
git add *
git commit -m "¡Primer commit!"
```

Este siguiente comando enviará tus archivos a la cadena, te pedirá tu frase semilla, sólo pégala en la terminal y presiona Enter:
```sh
git push origin master
```

Ahora has creado un nuevo repositorio git local, has añadido algunos archivos, has enlazado con el IPS que has creado en la cadena y has enviado tu commit local a la cadena.

Para demostrar que realmente está en la cadena, ve a un nuevo directorio y clona el repositorio git de la cadena usando el siguiente comando:
``sh
git clone inv4://0 clonado
```

Ahora puedes navegar dentro de este repositorio clonado y verificar que es el mismo que has empujado.
