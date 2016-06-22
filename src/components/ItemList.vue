<template>
  <group>
    <template v-for="item in shownItems">
      <item v-bind:item-key=item['.value']></item>
    </template>
  </group>
</template>

<script>
import Group from 'vux-components/group';
import Item from './Item';
import { checkVisible } from '../util';

export default {
  props: ['listType', 'items'],

  data() {
    return {
      // note: changing this line won't causes changes
      // with hot-reload because the reloaded component
      // preserves its current state and we are modifying
      // its initial state.
      maxItems: 20,
    };
  },

  components: {
    Item,
    Group,
  },

  computed: {
    shownItems() {
      return this.items.slice(0, this.maxItems);
    },
  },


  ready() {
    const self = this;
    const onScroll = () => {
      let elm = document.querySelectorAll('.weui_cell:last-child');
      if (elm.length === 1) {
        elm = elm[0];
        if ('getBoundingClientRect' in elm) {
          if (checkVisible(elm)) {
            if (self.maxItems < this.items.length) {
              self.maxItems += 10;
            } else {
              self.maxItems = this.items.length;
            }
          }
        }
      }
    };
    window.addEventListener('scroll', onScroll);
    onScroll();
  },
};
</script>

<!-- Add "scoped" attribute to limit CSS to this component only -->
<style scoped>
</style>
