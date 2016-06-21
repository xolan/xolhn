<template>
  <group>
    <template v-for="item in items25">
      <item v-bind:item-key=item['.value']></item>
    </template>
  </group>
</template>

<script>
import Group from 'vux-components/group';
import Item from './Item';

export default {
  props: ['listType', 'items'],

  data() {
    return {
      // note: changing this line won't causes changes
      // with hot-reload because the reloaded component
      // preserves its current state and we are modifying
      // its initial state.
    };
  },

  components: {
    Item,
    Group,
  },

  computed: {
    items25() {
      return this.items.slice(0, 10);
    },
  },
};
</script>

<!-- Add "scoped" attribute to limit CSS to this component only -->
<style scoped>
</style>
