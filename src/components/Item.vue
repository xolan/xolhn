<template>
  <template v-if="isLoading">
    <cell v-bind:title="getTitle">
      <spinner :type="lines"></spinner>
    </cell>
  </template>
  <template v-else>
    <cell
      v-bind:title="getTitle"
      v-bind:inline-desc="Derp"
      v-bind:is-link="getIsLink"
      :link="itemKey.toString()" />
  </template>
</template>

<script>
import Firebase from 'firebase';
import Cell from 'vux-components/cell';
import Spinner from 'vux-components/spinner';

export default {
  props: ['itemKey'],

  data() {
    return {
      // note: changing this line won't causes changes
      // with hot-reload because the reloaded component
      // preserves its current state and we are modifying
      // its initial state.
      itemData: {
        loading: true,
      },
    };
  },

  components: {
    Cell,
    Spinner,
  },

  computed: {
    isLoading() {
      if (this.itemData) {
        if (this.itemData.loading) {
          return this.itemData.loading;
        }
        return false;
      }
      return true;
    },
    getTitle() {
      if (!this.isLoading) {
        return this.itemData.title;
      }
      return `Loading (${this.itemKey})`;
    },
    getIsLink() {
      if (!this.isLoading) {
        if (this.itemData.url) {
          return true;
        }
      }
      return false;
    },
  },

  ready() {
    this.$bindAsObject('itemData', new Firebase(`https://hacker-news.firebaseio.com/v0/item/${this.itemKey}`));
  },
};
</script>

<!-- Add "scoped" attribute to limit CSS to this component only -->
<style scoped>
</style>
