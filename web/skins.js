// Presentation-only device skins for the web client.
//
// Keep emulation, input mapping, and rendering out of this module.  A skin
// only supplies CSS custom properties that position the existing screen and
// touch controls, following the same separation used by ProjectABE's skins.

export const SKINS = {
  arduboy: {
    label: 'Arduboy',
    description: 'Classic handheld',
    className: 'skin-arduboy',
  },
  microcard: {
    label: 'Microcard',
    description: 'Compact landscape',
    className: 'skin-microcard',
  },
  tama: {
    label: 'Tama',
    description: 'Portrait handheld',
    className: 'skin-tama',
  },
  pipboy: {
    label: 'Pipboy 3000',
    description: 'Retro terminal',
    className: 'skin-pipboy',
  },
  pipboymkiv: {
    label: 'Pipboy Mk IV',
    description: 'Wrist terminal',
    className: 'skin-pipboy-mkiv',
  },
};

export const DEFAULT_SKIN = 'arduboy';

export function getSkin(name) {
  return SKINS[name] ? name : DEFAULT_SKIN;
}
