import color from 'color';

const vars = {
  colors: {
    background: '#333333',
    brand: '#ff6600',
    accent: '#1D84B5',
    alt_1: '#444444',
    alt_2: '#555555',
    levels: {},
  },
};

const colorLevelsBase = color(vars.colors.background);
const colorLevelsMix = color(vars.colors.brand);

for (let index = 0; index < 11; index++) {
  vars.colors.levels[index.toString()] = colorLevelsBase.clone().mix(
      colorLevelsMix.clone(), (1 - (index / 10))
      ).hexString();
}

export default {
  vars,
};
