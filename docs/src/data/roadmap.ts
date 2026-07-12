export type Status = 'done' | 'next' | 'planned';

export interface Stage {
  n: string;
  slug: string;
  title: string;
  tagline: string;
  status: Status;
}

export interface Phase {
  id: string;
  name: string;
  goal: string;
  stages: Stage[];
}

export const statusLabel: Record<Status, string> = {
  done: 'concluído',
  next: 'próximo',
  planned: 'planejado',
};

export const phases: Phase[] = [
  {
    id: '0',
    name: 'Fase 0 · Fundações',
    goal: 'O que já existe: janela, framebuffer e matemática 3D.',
    stages: [
      {
        n: '00',
        slug: '/stages/00-fundacoes',
        title: 'Fundações: janela, framebuffer e math',
        tagline: 'Ponto de partida — o cubo wireframe já gira na tela.',
        status: 'done',
      },
    ],
  },
  {
    id: 'A',
    name: 'Fase A · Rasterizador software',
    goal: 'Fechar um pipeline de software completo antes de pensar em GPU.',
    stages: [
      {
        n: '01',
        slug: '/stages/01-rasterizacao',
        title: 'Triângulo preenchido + Z-buffer',
        tagline: 'Do arame ao sólido: superfícies opacas com profundidade correta.',
        status: 'next',
      },
      {
        n: '02',
        slug: '/stages/02-culling-clipping',
        title: 'Back-face culling + clipping de frustum',
        tagline: 'Não desenhar o que não se vê e cortar o que sai da tela.',
        status: 'planned',
      },
      {
        n: '03',
        slug: '/stages/03-luz-textura',
        title: 'Iluminação + texturização perspectiva-correta',
        tagline: 'Sombreamento flat/Gouraud e UVs sem distorção.',
        status: 'planned',
      },
    ],
  },
  {
    id: 'B',
    name: 'Fase B · Interação',
    goal: 'Controlar a cena em tempo real.',
    stages: [
      {
        n: '04',
        slug: '/stages/04-input-camera',
        title: 'Input acumulado + câmera livre',
        tagline: 'Estado de teclado/mouse e uma câmera FPS que anda pela cena.',
        status: 'planned',
      },
    ],
  },
  {
    id: 'C',
    name: 'Fase C · Núcleo ECS',
    goal: 'O coração da lib: transformar o renderer monolítico numa arquitetura data-oriented.',
    stages: [
      {
        n: '05',
        slug: '/stages/05-ecs-fundamentos',
        title: 'Fundamentos de ECS + entidades',
        tagline: 'Por que ECS, e entidades como índices geracionais.',
        status: 'planned',
      },
      {
        n: '06',
        slug: '/stages/06-ecs-storage',
        title: 'Armazenamento de componentes',
        tagline: 'Sparse-set: inserir, remover e iterar componentes rápido.',
        status: 'planned',
      },
      {
        n: '07',
        slug: '/stages/07-ecs-sistemas',
        title: 'Sistemas, scheduler e queries',
        tagline: 'World, queries tipadas e execução ordenada de sistemas.',
        status: 'planned',
      },
      {
        n: '08',
        slug: '/stages/08-ecs-renderer',
        title: 'Portar o renderer para o ECS',
        tagline: 'Cada objeto vira entidade; render vira um sistema.',
        status: 'planned',
      },
    ],
  },
  {
    id: 'D',
    name: 'Fase D · Cena & Assets',
    goal: 'Cenas hierárquicas e um pipeline de conteúdo real.',
    stages: [
      {
        n: '09',
        slug: '/stages/09-scene-graph',
        title: 'Transform hierárquico + scene graph',
        tagline: 'Pai/filho, transform local vs. global, propagação.',
        status: 'planned',
      },
      {
        n: '10',
        slug: '/stages/10-assets-blender',
        title: 'Assets, handles e pipeline Blender/glTF',
        tagline: 'Carregar glTF, gerenciar handles e importar do Blender.',
        status: 'planned',
      },
    ],
  },
  {
    id: 'E',
    name: 'Fase E · GPU',
    goal: 'Sair da CPU e desenhar de verdade na placa de vídeo.',
    stages: [
      {
        n: '11',
        slug: '/stages/11-wgpu',
        title: 'Migrar para a GPU com wgpu',
        tagline: 'Surface, pipeline, buffers e shaders WGSL.',
        status: 'planned',
      },
      {
        n: '12',
        slug: '/stages/12-pbr',
        title: 'Materiais PBR + iluminação real',
        tagline: 'Metallic-roughness, normal maps e múltiplas luzes.',
        status: 'planned',
      },
    ],
  },
  {
    id: 'F',
    name: 'Fase F · 2D',
    goal: 'A camada 2D em cima do mesmo ECS.',
    stages: [
      {
        n: '13',
        slug: '/stages/13-render-2d',
        title: 'Renderer 2D: sprites, batching e texto',
        tagline: 'Sprite batching, câmera 2D, atlas e fontes.',
        status: 'planned',
      },
    ],
  },
  {
    id: 'G',
    name: 'Fase G · Sistemas de jogo',
    goal: 'O que transforma um renderer em uma engine de jogo.',
    stages: [
      {
        n: '14',
        slug: '/stages/14-gameplay',
        title: 'Input actions, áudio, física e animação',
        tagline: 'Mapeamento de ações, som, colisão/rapier e keyframes.',
        status: 'planned',
      },
    ],
  },
  {
    id: 'H',
    name: 'Fase H · Ferramentas & Inovação',
    goal: 'Onde a Colibri deixa de ser "mais uma engine".',
    stages: [
      {
        n: '15',
        slug: '/stages/15-ferramentas',
        title: 'Editor (egui) + hot-reload',
        tagline: 'Inspector de entidades e recarga de assets/sistemas a quente.',
        status: 'planned',
      },
      {
        n: '16',
        slug: '/stages/16-inovacao',
        title: 'A inovação da Colibri',
        tagline: 'Live-link com Blender, determinismo, rollback e mais.',
        status: 'planned',
      },
    ],
  },
];

export const allStages: Stage[] = phases.flatMap((p) => p.stages);
