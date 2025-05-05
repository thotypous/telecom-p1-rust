# Modem dial-up

## Introdução

Nesta prática, vamos implementar um modem dial-up compatível com o padrão [V.21](https://www.itu.int/rec/dologin_pub.asp?lang=e&id=T-REC-V.21-198811-I!!PDF-E&type=items).


## Dependências

* Instale o ambiente de desenvolvimento Rust usando o [rustup](https://rustup.rs).

* Use um IDE como o [neovim](https://rsdlt.github.io/posts/rust-nvim-ide-guide-walkthrough-development-debug/) ou o [vscode](https://code.visualstudio.com/docs/languages/rust) com rust-analyzer para editar o código.


## Testando o código

Execute os testes do projeto usando o próprio IDE, ou se preferir use o script padrão do robô de correção (`./run-grader`).


## Passo-a-passo da implementação

### UART

Primeiro, você deve trabalhar no arquivo `uart.rs` para implementar um receptor de UART. Uma boa referência para entender o que deve ser feito é a [*application note* da Maxim](https://www.analog.com/en/technical-articles/determining-clock-accuracy-requirements-for-uart-communications.html). A diferença é que lá se discute um receptor com clock 16 vezes superior ao *baud rate*, ao passo que aqui temos um clock muito mais rápido — 147 ou 160 vezes superior ao *baud rate* — já que vamos usar a taxa de amostragem nativa da maioria das interfaces de áudio (44100 Hz ou 48000 Hz).

A função `UartRx::put_samples` recebe um `buffer` contendo um sinal binário (cada elemento do `buffer` é 0 ou 1) amostrado à mesma taxa da interface de áudio. O argumento `samples_per_symbol` informa o número de amostras correspondente à duração esperada de cada símbolo, que geralmente vale 147 ou 160 (depende da taxa de amostragem suportada pela interface de áudio). O `buffer` pode ter qualquer tamanho e você deve lidar com esse fato. A forma mais fácil é fazer um loop iterando por cada amostra individual de `buffer` e nunca tentar olhar para as amostras futuras, nem olhar para amostras passadas de forma direta. Ou seja, se você precisar consultar amostras passadas, armazene-as de alguma forma como atributo da struct `UartRx`, mas nunca faça coisas como `buffer[i-1]`. Seguindo essas regras básicas, o seu código vai funcionar como uma máquina de estados fácil de entender e você não vai precisar ficar tratando casos especiais.

Sempre que você terminar de receber um byte completo, chame `self.to_pty.send(byte_completo).unwrap();`.

#### Sugestão de implementação

Na *application note*, discute-se um receptor que espera uma amostra com nível lógico baixo no início do *start bit*, e três amostras com nível lógico baixo no meio do *start bit*. Como nosso clock é maior, precisamos fazer a proporção — o análogo seria esperar `3*samples_per_symbol/16` amostras com nível lógico baixo no meio do *start bit*. Para tolerar um pouco de erro, eu sugiro verificar se pelo menos 5/6 dessas amostras têm nível lógico baixo. Se, além disso, a amostra que estaria no início do *start bit* tiver nível lógico baixo, considere que você achou o *start bit*. Quando isso acontecer, comece a contar múltiplos de `samples_per_symbol` amostras a partir do meio do *start bit*, acrescentando o valor de cada amostra situada nesses múltiplos ao byte que está sendo recebido. Ao chegar no *stop bit*, passe o byte recebido para `self.to_pty` e volte sua lógica para o estado ocioso, no qual você deve procurar pelo próximo *start bit*.

#### Testes do UART

Há quatro testes para o UART — trivial, unsync, noisy e noisy\_unsync. O teste trivial não introduz ruído nem diferença entre os clocks do receptor e transmissor. Você pode tentar passar nesse teste primeiro para se ambientar com o código, o que deve ser possível mesmo se você começar a contagem no início do *start bit*. Mas se você começar a contagem no meio do *start bit* e estiver fazendo tudo certo, você deve passar em todos os quatro testes de uma vez só.


### V21

Uma vez funcionando o UART, você deve trabalhar no arquivo `v21.rs` para implementar um demodulador FSK.

A função `V21RX::demodulate` recebe um array `in_samples` contendo o sinal proveniente da interface de áudio. As amostras de entrada podem ter uma infinidade de níveis de tensão (a maioria das interfaces de áudio têm conversores de 24 bits, ou seja, há 2²⁴ níveis de tensão possíveis). O objetivo do demodulador é transformar esse sinal em um sinal de apenas dois níveis.

Para cada uma das amostras de entrada em `in_samples`, o demodulador deve gerar uma amostra de saída em `out_samples` valendo 0 ou 1, indicando se, naquele momento, é mais provável que o demodulador esteja processando áudio pertencente a um símbolo 0 ou 1, respectivamente.

O demodulador não precisa se preocupar em delimitar o início e o fim de cada símbolo — isso ficará a cargo do UART.

O sinal de entrada pode ter qualquer tamanho e você deve lidar com isso, similar ao que acontece na implementação do UART (realizada no passo anterior da prática). A dica é a mesma de antes: tratar uma amostra por vez, nunca olhar para o futuro, e nunca usar diretamente o array de entrada para olhar para o passado.

#### Arquitetura sugerida para o demulador

Faça dois filtros passa-bandas, um para o tom de marca, outro para o tom de espaço. Subtraia a saída de um dos filtros passa-bandas da saída do outro, e filtre essa diferença com um passa-baixas. Quando a diferença filtrada indicar uma energia maior na frequência de espaço, insira um 0 no buffer de saída. Caso contrário, insira um 1.

Implemente uma estratégia para detectar a ausência de uma portadora (ou seja, quando não existe nem tom de marca nem tom de espaço). Enquanto não houver portadora, insira sempre 1 no buffer de saída.

A seção "Demodulando um sinal FSK" [deste notebook Jupyter](https://colab.research.google.com/drive/1tjileevEYGz6IMGCzqgFq9Sy4GvYtBuL?usp=sharing) ensina a fazer tudo isso em Python de forma *offline*. Traduzir esse código para Rust não é muito difícil. Mas atente-se para o fato de que você deve transformá-lo em um código que opere de forma *online*, ou seja, tratando uma amostra de entrada por vez e guardando todo estado que for necessário como atributo da struct `V21RX`.

#### Projeto do filtro passa-baixas

Como a taxa de amostragem só é conhecida em tempo de execução, eu recomendo usar a biblioteca `fundsp` (já incluída no esqueleto do código) para projetar o filtro passa-baixas. Crie o filtro da seguinte forma:

```rust
let mut lowpass = fundsp::filter::ButterLowpass::new(300.);
lowpass.set_sample_rate(1. / sampling_period as f64);
```

A criação do filtro deve ser realizada uma única vez, na função `V21RX::new`, armazenando-o como atributo da struct `V21RX`. Para processar uma amostra com esse filtro, chame `*self.lowpass.tick(&Frame::from([amostra_entrada])).first().unwrap()`. Essa expressão processa a amostra de entrada e retorna uma amostra de saída do filtro.

#### Testes do V21

Os testes de V21 traçam o gráfico de BER vs Eb/N0 e comparam alguns pontos desse gráfico com valores de referência. Se você implementar tudo certo, deve passar nos dois testes de uma vez só.

O teste unsync usa relógios ligeiramente diferentes no transmissor e no receptor. Uma falha nesse teste provavelmente não indica um erro de implementação do V21, mas sim do UART, que é responsável pela sincronização.


### Testes de bancada

De 10 pontos da nota, 8 são atribuídos automaticamente pelo robô de correção.

Para conseguir os 2 pontos restantes, seu grupo deve submeter a implementação a um teste de bancada, interoperando com [um modem dial-up disponível comercialmente](https://pt.aliexpress.com/item/2032456154.html).

Para isso, é necessário saber como **usar** o modem, e não só como executar os testes automatizados.

Como cada ponta do enlace usa um par de frequências diferente para transmitir, a convenção é escolher esse par de frequências de acordo com o papel da ponta na chamada telefônica. Se foi quem discou (em inglês, *caller*), utiliza-se um par de frequências; se foi quem recebeu a chamada (em inglês, *answerer*), utiliza-se o outro. Por isso, será necessário passar essa informação ao modem.

#### Linux

Em um terminal, execute:

```bash
./modem   # na outra ponta, ./modem --answer
```

Se quiser que o modem use uma interface de áudio diferente da `default`, passe o nome da interface pelas opções `--rxdev` e `--txdev`. Para listar os nomes das interfaces de áudio disponíveis, use o exemplo [enumerate](https://github.com/RustAudio/cpal/blob/master/examples/enumerate.rs) da biblioteca cpal.

Em outro terminal, execute o picocom passando o dispositivo informado na saída do modem:

```bash
picocom -b 115200 --echo /dev/pts/N
```

Com o picocom, você pode trocar mensagens de texto diretamente com a outra ponta.

Em vez de usar o picocom, você também pode usar o slattach para subir uma interface SLIP:

```bash
sudo slattach -v -p slip /dev/pts/N

# em outro terminal:
sudo ifconfig sl0 192.168.123.x pointopoint 192.168.123.y
```

ou até mesmo usar seu modem em conjunto com as suas [práticas da disciplina de Redes](https://github.com/thotypous/redes-s1)!


#### Windows

Antes de usar o modem no Windows, você precisa instalar o [com0com](https://sourceforge.net/projects/com0com/files/latest/download). Depois de instalar, reinicie o computador e faça uma atualização do driver pelo Windows Update. Você provavelmente terá de habilitar essa atualização manualmente na lista de atualizações opcionais de drivers. **Isso é muito importante**, pois a assinatura do driver que vem no instalador do com0com expirou, então o driver não funciona a menos que você o atualize pelo Windows Update.

Ao contrário do Linux, em que o subsistema pty permite alocar portos seriais virtuais dinamicamente, com o com0com o nosso modem precisa conectar a um porto serial virtual previamente configurado.

Durante a instalação do com0com, se você não tiver alterado nenhuma opção, ele terá criado um par de portos seriais virtuais COM3/COM4. O padrão do modem é conectar em COM3, de forma que ele ficará acessível para você na COM4. Se você precisar mudar isso, pode passar a opção `-s PORTO` para o modem.

Abra um Prompt do MS-DOS ou um terminal do PowerShell e execute `modem` ou `modem --answer`. Também é possível usar as opções `--rxdev` e `--txdev` para escolher a interface de áudio usada pelo modem, da mesma forma que na versão Linux.

Utilize o Putty para conectar-se à COM4 se quiser trocar mensagens de texto diretamente com a outra ponta.

Utilize o discador do Windows se quiser subir uma interface de rede. Infelizmente, o Windows 7 parece ter sido a última versão do Windows a suportar SLIP. Mas você pode tentar usar PPP. Aceito pull requests com um passo-a-passo de como fazer isso :D


#### Modem comercialmente disponível

Os modems comercialmente disponíveis também apresentam-se ao usuário como dispositivos seriais mas, como eles suportam uma diversidade de protocolos e modos de operação, é necessário operá-los usando [comandos AT](https://www.thinkpenguin.com/files/CX930xx-manual.pdf) (também conhecidos como comandos Hayes).

Conecte-se ao dispositivo do modem utilizando o picocom (no Linux), Putty (no Windows) ou outro software que funcione como um terminal serial. No Linux, modems USB costumam ser detectados como `/dev/ttyUSB0` ou `/dev/ttyACM0`.

Se você estiver usando o simulador de linha telefônica (em vez do PABX), use o seguinte comando para que o modem ignore a ausência de um tom de discagem:

```
ATX3
```

Use o seguinte comando para colocar o modem em modo V.21:

```
AT+MS=V21,0
```

Para estabelecer a conexão, caso você esteja na ponta que efetua a chamada, use o comando:

```
ATDT1
```

Acima, `1` é um número de telefone, que será chamado usando tons [DTMF](https://en.wikipedia.org/wiki/Dual-tone_multi-frequency_signaling). Perceba que o modem que nós implementamos não possui essa funcionalidade de chamar um número — ele assume que a chamada telefônica já está estabelecida no momento que ele começa a operar.

Caso você esteja na ponta que recebe a chamada, use o comando:

```
ATA
```

A partir daí, você pode trocar mensagens de texto diretamente com a outra ponta.

Se estiver no picocom e quiser subir uma interface SLIP, use as teclas Ctrl+A seguidas de Ctrl+Q para sair do picocom sem desligar a chamada (sem "colocar o telefone no gancho"). Em seguida, utilize o comando `slattach`, passando o dispositivo serial do modem.
