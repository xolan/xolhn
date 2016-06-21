<template>
  <item-list list-type="Top Stories" v-bind:items=topstories></item-list>
</template>

<script>
import Firebase from 'firebase';
import ItemList from './ItemList';

export default {
  components: {
    ItemList,
  },

  firebase: {
    topstories: new Firebase('https://hacker-news.firebaseio.com/v0/topstories'),
  },
};
</script>

<style>
</style>
